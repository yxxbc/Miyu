//! 图库面板的后端动作:列表 / 图片直出 / 上传 / 改描述 / 删除。
//!
//! 与工具层共用 [`super::store`] 的索引读写,但**入口不同**:工具层由模型调用,
//! 面板由人点。两条路同时动 `index.json` 是现实存在的(她正在存图,你正在面板里
//! 改描述),所以读-改-写整段套一把进程级锁——索引是整份替换的,没有锁就是
//! 后写的把先写的整份盖掉。
//!
//! 锁在 [`store::with_index`],按库目录分:两个人格的库互不阻塞。

use super::store::{self, with_index, AlbumEntry};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn entry_json(entry: &AlbumEntry, root: &Path) -> Value {
    let path = entry.path(root);
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).ok();
    json!({
        "id": entry.id,
        "name": entry.name,
        "description": entry.description,
        "tags": entry.tags,
        "file": entry.file,
        "source": entry.source,
        "saved_at": entry.saved_at,
        "missing": !path.is_file(),
        "bytes": bytes,
    })
}

pub fn dashboard_list(root: &Path) -> Result<Value> {
    let entries = store::load_index(root);
    let items: Vec<Value> = entries
        .iter()
        .map(|entry| entry_json(entry, root))
        .collect();
    let missing = items
        .iter()
        .filter(|item| item["missing"].as_bool() == Some(true))
        .count();
    let bytes: u64 = items.iter().filter_map(|item| item["bytes"].as_u64()).sum();
    Ok(json!({
        "ok": true,
        "root": root.display().to_string(),
        "total": items.len(),
        "missing": missing,
        "bytes": bytes,
        "items": items,
    }))
}

/// (路径, mime)。id 不存在或文件已不在时给 `None`,由调用方翻 404。
pub fn dashboard_image(root: &Path, id: &str) -> Option<(PathBuf, &'static str)> {
    let entries = store::load_index(root);
    let entry = entries.iter().find(|entry| entry.id == id)?;
    let path = entry.path(root);
    if !path.is_file() {
        return None;
    }
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        "tiff" => "image/tiff",
        _ => "image/jpeg",
    };
    Some((path, mime))
}

pub struct DashboardUpload {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    /// 浏览器给的原始文件名,只用来兜底猜后缀与名字。
    pub filename: String,
}

/// 看头几个字节定后缀。浏览器给的文件名可以撒谎,魔数不会——而且面板上传走的
/// 是裸 body,本来就没有可信的后缀。
fn sniff_suffix(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("bmp");
    }
    // HEIC/HEIF:ftyp box 里的 major brand。
    if bytes.len() > 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if brand == b"heic" || brand == b"heix" || brand == b"mif1" || brand == b"heif" {
            return Some("heic");
        }
    }
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Some("tiff");
    }
    None
}

pub fn dashboard_add(root: &Path, upload: DashboardUpload, bytes: &[u8]) -> Result<Value> {
    let Some(suffix) = sniff_suffix(bytes) else {
        bail!(
            "认不出这是图片(支持 {});别的类型请先转换",
            store::SUFFIXES.join(" / ")
        );
    };
    with_index(root, |entries| {
        let id = store::new_id(entries, &upload.filename);
        std::fs::create_dir_all(root).with_context(|| format!("建图库目录 {}", root.display()))?;
        let file = format!("{id}.{suffix}");
        std::fs::write(root.join(&file), bytes)
            .with_context(|| format!("写入 {}", root.join(&file).display()))?;
        let name = if upload.name.trim().is_empty() {
            Path::new(&upload.filename)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .filter(|stem| !stem.trim().is_empty())
                .unwrap_or_else(|| id.clone())
        } else {
            upload.name.trim().to_string()
        };
        let entry = AlbumEntry {
            id: id.clone(),
            name,
            description: upload.description.trim().to_string(),
            tags: upload.tags,
            file,
            source: "webui".to_string(),
            saved_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        let value = entry_json(&entry, root);
        entries.insert(0, entry);
        Ok(json!({ "ok": true, "item": value, "total": entries.len() }))
    })
}

#[derive(Default)]
pub struct DashboardPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub fn dashboard_update(root: &Path, id: &str, patch: DashboardPatch) -> Result<Value> {
    with_index(root, |entries| {
        let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
            bail!("图库里没有 {id} 这条");
        };
        if let Some(name) = patch.name {
            let name = name.trim();
            if name.is_empty() {
                bail!("名字不能是空的");
            }
            entry.name = name.to_string();
        }
        if let Some(description) = patch.description {
            entry.description = description.trim().to_string();
        }
        if let Some(tags) = patch.tags {
            entry.tags = tags
                .into_iter()
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .take(20)
                .collect();
        }
        let value = entry_json(entry, root);
        Ok(json!({ "ok": true, "item": value }))
    })
}

/// `hard` 才删文件;默认只摘条目,图还在盘上(与工具层 delete 同一口径)。
pub fn dashboard_delete(root: &Path, id: &str, hard: bool) -> Result<Value> {
    with_index(root, |entries| {
        let Some(position) = entries.iter().position(|entry| entry.id == id) else {
            bail!("图库里没有 {id} 这条");
        };
        let entry = entries.remove(position);
        let mut removed_file = false;
        if hard {
            let path = entry.path(root);
            if path.is_file() {
                std::fs::remove_file(&path).with_context(|| format!("删除 {}", path.display()))?;
                removed_file = true;
            }
        }
        Ok(json!({
            "ok": true,
            "id": entry.id,
            "file_removed": removed_file,
            "total": entries.len(),
        }))
    })
}
