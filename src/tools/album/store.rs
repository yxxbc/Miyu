//! 图库的落盘层:目录解析、索引读写、条目模型。
//!
//! 落点 `home/<用户>/pictures/album/<人格 scope>/`:
//!
//! - 藏在 `pictures` 下面是**故意的**——`gqy export` 的清单里有 `home.pictures`,
//!   图库于是白拿一份导出,不用动导出清单。
//! - 按人格 scope 再分一层(用户裁定,与表情包同口径):换人格不串图。scope 取
//!   [`AppConfig::active_persona_scope`],成员私有人格是 `home-<用户>-<slug>`,
//!   两个人的同名人格不会并成一库。
//! - 成员回合落成员自己家(`member_home_dir`),不是管理员的 `data/pictures`
//!   ——后者成员被 Landlock 挡在外面,写得进去也读不回来。
//!
//! 索引是目录里的 `index.json`,整份读写、原子替换。图库是**人手一张一张攒**的
//! 量级(几十到几百),没有上数据库的理由;真长到需要索引结构再说。

use crate::config::AppConfig;
use crate::paths::GqyPaths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub(crate) const INDEX_FILE: &str = "index.json";

/// 认的图片后缀。`sips`/浏览器都能处理,`print_image` 也认。
pub(crate) const SUFFIXES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "heif", "tiff",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumEntry {
    /// 8 位十六进制,库内唯一,也是落盘文件名的前缀。
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 库目录下的文件名(`<id>.<ext>`),不存绝对路径——换机器/改家目录后仍然有效。
    pub file: String,
    /// 收进来时的来源路径,只作痕迹,不参与解析。
    #[serde(default)]
    pub source: String,
    pub saved_at: String,
}

impl AlbumEntry {
    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(&self.file)
    }
}

/// 当前回合该用哪个图库目录。
pub fn album_root(config: &AppConfig, paths: &GqyPaths) -> PathBuf {
    let pictures_base = match config.member_home_dir() {
        Some(home) => home.join("pictures"),
        None => paths.pictures_dir.clone(),
    };
    pictures_base.join("album").join(persona_scope(config))
}

/// 人格 scope → 目录名。非字母数字一律折成 `-`,空了退回 `default`。
fn persona_scope(config: &AppConfig) -> String {
    let raw = config.active_persona_scope();
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else if ch.is_alphanumeric() {
                ch // 中文人格名原样留着,文件系统认
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed
    }
}

pub fn load_index(root: &Path) -> Vec<AlbumEntry> {
    let path = root.join(INDEX_FILE);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    // 索引坏了不该让整个工具哑掉:记 warn,当空库继续,旧文件还在盘上。
    match serde_json::from_str::<Vec<AlbumEntry>>(&raw) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(error = %error, path = %path.display(), "album index unreadable");
            Vec::new()
        }
    }
}

/// 整份写回,先写 `.tmp` 再 rename——中途挂掉不会留半个索引。
pub fn save_index(root: &Path, entries: &[AlbumEntry]) -> Result<()> {
    std::fs::create_dir_all(root).with_context(|| format!("建图库目录 {}", root.display()))?;
    let path = root.join(INDEX_FILE);
    let tmp = root.join(format!("{INDEX_FILE}.tmp"));
    let body = serde_json::to_string_pretty(entries)?;
    std::fs::write(&tmp, body).with_context(|| format!("写 {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("替换 {}", path.display()))?;
    Ok(())
}

/// 库目录 → 锁。只增不删:一次会话里库的个数是人格数量级。
fn index_lock(root: &Path) -> &'static Mutex<()> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, &'static Mutex<()>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = locks.lock().unwrap();
    guard
        .entry(root.to_path_buf())
        .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
}

/// 读-改-写一次索引,整段持锁。
///
/// 索引是**整份替换**的,而动它的现在有两条路:模型调 `album` 工具、人在 WebUI
/// 面板上改。没有这把锁,两边并发时后写的会把先写的整份盖掉——她刚存进去的图
/// 会在你保存描述的那一刻凭空消失。闭包返回 `Err` 时不落盘。
pub fn with_index<T>(
    root: &Path,
    edit: impl FnOnce(&mut Vec<AlbumEntry>) -> Result<T>,
) -> Result<T> {
    let lock = index_lock(root);
    let _guard = lock.lock().unwrap();
    let mut entries = load_index(root);
    let out = edit(&mut entries)?;
    save_index(root, &entries)?;
    Ok(out)
}

/// 按 id 精确找,找不到再按名字找(名字可能撞,调用方负责报歧义)。
pub fn find<'a>(entries: &'a [AlbumEntry], key: &str) -> Vec<&'a AlbumEntry> {
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return Vec::new();
    }
    let exact: Vec<_> = entries
        .iter()
        .filter(|entry| entry.id.to_lowercase() == key)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    entries
        .iter()
        .filter(|entry| entry.name.to_lowercase() == key)
        .collect()
}

pub fn new_id(entries: &[AlbumEntry], seed: &str) -> String {
    let mut base = blake3::hash(
        format!(
            "{seed}|{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
        .as_bytes(),
    )
    .to_hex()[..8]
        .to_string();
    while entries.iter().any(|entry| entry.id == base) {
        base = blake3::hash(format!("{base}x").as_bytes()).to_hex()[..8].to_string();
    }
    base
}

/// 把源文件收进库目录,返回库内文件名。
pub fn take_in(root: &Path, source: &Path, id: &str) -> Result<String> {
    let suffix = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if !SUFFIXES.contains(&suffix.as_str()) {
        bail!(
            "not an image file (.{suffix}); supported: {}",
            SUFFIXES.join(", ")
        );
    }
    std::fs::create_dir_all(root).with_context(|| format!("建图库目录 {}", root.display()))?;
    let stored = format!("{id}.{suffix}");
    std::fs::copy(source, root.join(&stored))
        .with_context(|| format!("复制 {} 进图库", source.display()))?;
    Ok(stored)
}
