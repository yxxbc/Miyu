//! 图库:存得住、找得到、发得出的普通图片库。
//!
//! **和表情包库的分工**是这件工具存在的全部理由。表情包在「该有个反应」的时候是
//! **自动挑**的,把人格立绘、用户让留的截图塞进去会污染那套挑选;反过来,`print_image`
//! 本来就能显示任意路径的图,所以缺的从来不是「发图」,是「存和找」。
//!
//! 按人格分库(用户裁定,与表情包同口径),落点与索引见 [`store`]。

mod dashboard;
mod store;

pub use dashboard::{
    dashboard_add, dashboard_delete, dashboard_image, dashboard_list, dashboard_update,
    DashboardPatch, DashboardUpload,
};
pub use store::{album_root, AlbumEntry};

use super::{ToolProgress, ToolRegistry, ToolSpec};
use crate::config::AppConfig;
use crate::paths::GqyPaths;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn register(registry: &mut ToolRegistry, config: AppConfig, paths: GqyPaths) {
    registry.register(
        ToolSpec::new_with_progress(
            "album",
            "占位描述(注册时被 descriptions/album.json 整体覆盖)",
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["save", "search", "list", "show", "delete"],
                        "description": "save takes a picture in; search finds by keyword; list shows everything newest first; show sends one; delete drops an entry."
                    },
                    "path": { "type": "string", "description": "Required for save: the image file to take in. Absolute or ~/ path." },
                    "name": { "type": "string", "description": "For save: short human name. Defaults to the file name." },
                    "description": { "type": "string", "description": "For save: what is in the picture. Write a real one — search only matches name, description and tags." },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "For save: tags for later search." },
                    "query": { "type": "string", "description": "Required for search: keyword matched against name, description and tags." },
                    "id": { "type": "string", "description": "Required for show and delete: the entry id from search or list. A name works too when unambiguous." },
                    "hard_delete": { "type": "boolean", "description": "For delete: also remove the image file. Default false (entry only)." },
                    "limit": { "type": "integer", "description": "For list and search: max entries, 1-50. Default 20." }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            move |args, progress| {
                let config = config.clone();
                let paths = paths.clone();
                async move { dispatch(args, progress, &config, &paths) }
            },
        )
        .writes()
        .with_groups(vec!["images".to_string()])
        .with_stub_example(r#"{"action":"search","query":"立绘"}"#),
    );
}

fn dispatch(
    args: Value,
    progress: ToolProgress,
    config: &AppConfig,
    paths: &GqyPaths,
) -> Result<String> {
    let root = album_root(config, paths);
    // 写动作整段持锁(面板也在动同一份 index.json);读动作直接读当下那份。
    match args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "save" => store::with_index(&root, |entries| save(&args, &root, entries)),
        "delete" => store::with_index(&root, |entries| delete(&args, &root, entries)),
        "search" => search(&args, &root, &store::load_index(&root)),
        "list" => list(&args, &root, &store::load_index(&root)),
        "show" => show(&args, &root, &store::load_index(&root), &progress),
        other => bail!("unknown action: {other}; expected save, search, list, show or delete"),
    }
}

fn limit_of(args: &Value) -> usize {
    args.get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 50) as usize
}

fn save(args: &Value, root: &PathBuf, entries: &mut Vec<AlbumEntry>) -> Result<String> {
    let raw = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty());
    let Some(raw) = raw else {
        bail!("action=save needs a path")
    };
    let source = crate::tools::workspace::expand_path(raw);
    if !source.is_file() {
        bail!("no such file: {}", source.display());
    }
    crate::tools::sandbox::guard_read(&source)?;

    let id = store::new_id(entries, &source.to_string_lossy());
    let file = store::take_in(root, &source, &id)?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            source
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| id.clone())
        });
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let tags: Vec<String> = args
        .get("tags")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .take(20)
                .collect()
        })
        .unwrap_or_default();

    entries.insert(
        0,
        AlbumEntry {
            id: id.clone(),
            name: name.clone(),
            description: description.clone(),
            tags,
            file,
            source: source.to_string_lossy().into_owned(),
            saved_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        },
    );

    let note = if description.is_empty() {
        "\n(no description — search will not find it; add one)"
    } else {
        ""
    };
    Ok(format!(
        "saved to the album: {name} (id {id}); {} pictures total{note}",
        entries.len()
    ))
}

fn search(args: &Value, root: &PathBuf, entries: &[AlbumEntry]) -> Result<String> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|query| !query.is_empty());
    let Some(query) = query else {
        bail!("action=search needs a query")
    };
    let words: Vec<String> = query
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == '，')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut hits: Vec<(usize, &AlbumEntry)> = entries
        .iter()
        .filter_map(|entry| {
            let haystack = format!(
                "{} {} {}",
                entry.name,
                entry.description,
                entry.tags.join(" ")
            )
            .to_lowercase();
            let score = words.iter().filter(|word| haystack.contains(*word)).count();
            (score > 0).then_some((score, entry))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    let shown: Vec<&AlbumEntry> = hits
        .into_iter()
        .map(|(_, entry)| entry)
        .take(limit_of(args))
        .collect();
    if shown.is_empty() {
        return Ok(format!("no album entry matches {query:?}"));
    }
    Ok(render(&shown, root, entries.len()))
}

fn list(args: &Value, root: &PathBuf, entries: &[AlbumEntry]) -> Result<String> {
    if entries.is_empty() {
        return Ok("the album is empty".to_string());
    }
    let shown: Vec<&AlbumEntry> = entries.iter().take(limit_of(args)).collect();
    Ok(render(&shown, root, entries.len()))
}

fn show(
    args: &Value,
    root: &PathBuf,
    entries: &[AlbumEntry],
    progress: &ToolProgress,
) -> Result<String> {
    let key = args.get("id").and_then(Value::as_str).unwrap_or_default();
    let matches = store::find(entries, key);
    match matches.len() {
        0 => bail!("no album entry for {key:?}; run action=search first and use the id it returns"),
        1 => {}
        n => bail!("{key:?} matches {n} entries; use the id instead of the name"),
    }
    let entry = matches[0];
    let path = entry.path(root);
    if !path.is_file() {
        bail!(
            "the file is gone: {}; the entry is stale, delete it with action=delete id={}",
            path.display(),
            entry.id
        );
    }
    let caption = if entry.description.is_empty() {
        entry.name.clone()
    } else {
        entry.description.clone()
    };
    progress.report_image(path, caption);
    Ok(format!("sent {} (id {})", entry.name, entry.id))
}

fn delete(args: &Value, root: &PathBuf, entries: &mut Vec<AlbumEntry>) -> Result<String> {
    let key = args.get("id").and_then(Value::as_str).unwrap_or_default();
    let (id, file) = {
        let matches = store::find(entries, key);
        match matches.len() {
            0 => bail!("no album entry for {key:?}; check the id with action=list"),
            1 => (matches[0].id.clone(), matches[0].file.clone()),
            n => bail!("{key:?} matches {n} entries; use the id instead of the name"),
        }
    };
    let hard = args
        .get("hard_delete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut removed_file = false;
    if hard {
        let path = root.join(&file);
        if path.is_file() {
            std::fs::remove_file(&path)?;
            removed_file = true;
        }
    }
    entries.retain(|entry| entry.id != id);
    Ok(format!(
        "deleted entry {id} ({}); {} pictures left",
        if removed_file {
            "file removed too"
        } else {
            "file kept"
        },
        entries.len()
    ))
}

fn render(shown: &[&AlbumEntry], root: &PathBuf, total: usize) -> String {
    let mut out = format!("{} of {total} pictures:\n", shown.len());
    for entry in shown {
        let missing = if entry.path(root).is_file() {
            ""
        } else {
            " [file missing]"
        };
        out.push_str(&format!("- {} {}{missing}\n", entry.id, entry.name));
        if !entry.description.is_empty() {
            out.push_str(&format!("  {}\n", entry.description));
        }
        if !entry.tags.is_empty() {
            out.push_str(&format!("  tags: {}\n", entry.tags.join(", ")));
        }
    }
    out
}
