//! 地图瓦片代理:`/api/map/tile`。
//!
//! **为什么不让浏览器直接去拿瓦片。** 与链接卡片是同一条理由:WebUI 的 CSP 是
//! `img-src 'self'`。放开它意味着每打开一张地图,用户的浏览器就要向瓦片服务器
//! 发几十个带 IP 与 Referer 的请求——一张地图一屏就是十几二十块瓦片,足够对面
//! 画出「谁、什么时候、在看哪里」。所以瓦片由 daemon 代取,浏览器只跟本机说话。
//!
//! **缓存的两难。** 用户的要求是「地图最好每次处于最新状态」,而公共瓦片服务器
//! (尤其 OSM 官方)的使用条款明确不欢迎把它当自家 CDN 刷。折中是磁盘缓存 +
//! TTL(默认 72 小时,`plugins.map.tile_ttl_hours`,0 = 不缓存):新修的路、改名
//! 的店最多隔三天就跟上,而反复看同一片区域不会一遍遍回源。缓存超过
//! `tile_cache_mb` 时按最旧的删。
//!
//! 坐标只认标准 XYZ:z 与 x/y 都要落在合法范围里,拼路径之前就挡住——这几个数
//! 直接进文件路径,放任负数或超界的值等于把路径拼接的口子敞开。

use crate::web::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// 再高的层级公共瓦片源基本也没有数据,而层级越深瓦片数量越爆炸。
const MAX_ZOOM: u32 = 19;
/// 单块瓦片的体积上限:正常几 KB 到几十 KB。
const MAX_TILE_BYTES: usize = 512 * 1024;
const TILE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
/// Nominatim / OSM 瓦片的使用条款都要求带一个能联系上的 User-Agent。
const OSM_USER_AGENT: &str = "GQY/0.5 (personal assistant; https://github.com/yxxbc/Miyu)";

#[derive(Deserialize)]
pub(in crate::web) struct TileQuery {
    #[serde(default)]
    p: String,
    z: u32,
    x: u32,
    y: u32,
}

fn upstream_url(provider: &str, z: u32, x: u32, y: u32) -> Option<String> {
    match provider {
        // 高德的中文路网瓦片(带注记)。四个域名轮着用是它自己的惯例,
        // 按瓦片坐标散列,同一块瓦片永远落同一个域名,缓存友好。
        "amap" => Some(format!(
            "https://webrd0{}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&scale=1&style=8&x={x}&y={y}&z={z}",
            (x + y) % 4 + 1
        )),
        "osm" => Some(format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png")),
        _ => None,
    }
}

fn cache_path(cache_dir: &std::path::Path, provider: &str, z: u32, x: u32, y: u32) -> PathBuf {
    cache_dir
        .join("map-tiles")
        .join(provider)
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"))
}

fn fresh(path: &std::path::Path, ttl: std::time::Duration) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    let age = meta.modified().ok()?.elapsed().ok()?;
    (age < ttl).then(|| std::fs::read(path).ok())?
}

/// 每写这么多块瓦片查一次总量。逐块 walk 整个缓存目录太贵,而瓦片是小文件、
/// 涨得慢,128 块(几 MB)一查足够。
const PRUNE_EVERY: u32 = 128;
static WRITES: AtomicU32 = AtomicU32::new(0);

/// 超出上限就按 mtime 从旧到新删到八成。删错了的代价只是重取一次。
fn prune(root: &std::path::Path, limit_bytes: u64) {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if let Ok(modified) = meta.modified() {
                files.push((modified, meta.len(), entry.path()));
            }
        }
    }
    let total: u64 = files.iter().map(|(_, size, _)| size).sum();
    if total <= limit_bytes {
        return;
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    let target = limit_bytes / 10 * 8;
    let mut left = total;
    for (_, size, path) in files {
        if left <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            left = left.saturating_sub(size);
        }
    }
}

pub(in crate::web) async fn map_tile(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<TileQuery>,
) -> std::result::Result<Response, ApiError> {
    require_auth(&headers, &state)?;
    let config = {
        let manager = state.manager.lock().unwrap();
        manager.config.plugins.map.clone()
    };
    if !config.enabled {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "map plugin disabled"));
    }
    let provider = if query.p.trim().is_empty() {
        "osm".to_string()
    } else {
        query.p.trim().to_ascii_lowercase()
    };
    let (z, x, y) = (query.z, query.x, query.y);
    if z > MAX_ZOOM {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "zoom out of range"));
    }
    // 2^z × 2^z 是该层级的瓦片网格;越界的 x/y 上游只会给 404,而它们要进路径。
    let span = 1u32 << z;
    if x >= span || y >= span {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "tile out of range"));
    }
    let Some(url) = upstream_url(&provider, z, x, y) else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "unknown tile provider"));
    };

    let ttl = std::time::Duration::from_secs(config.tile_ttl_hours.saturating_mul(3600));
    let path = cache_path(&state.paths.cache_dir, &provider, z, x, y);
    if !ttl.is_zero() {
        let probe = path.clone();
        let hit = tokio::task::spawn_blocking(move || fresh(&probe, ttl))
            .await
            .map_err(ApiError::internal)?;
        if let Some(bytes) = hit {
            return Ok(tile_response(bytes));
        }
    }

    let fetched = fetch_tile(&url, &provider).await;
    let bytes = match fetched {
        Ok(bytes) => bytes,
        Err(error) => {
            // 回源失败时,过期的那份仍然比一块空白强——地图上少一块瓦片是很显眼的
            // 破洞,而三天前的那块路网几乎一定还是对的。
            if let Ok(stale) = std::fs::read(&path) {
                return Ok(tile_response(stale));
            }
            tracing::debug!(error = %error, url = %url, "map tile fetch failed");
            return Err(ApiError::new(StatusCode::BAD_GATEWAY, "tile fetch failed"));
        }
    };

    if !ttl.is_zero() {
        let write_bytes = bytes.clone();
        let cache_root = state.paths.cache_dir.join("map-tiles");
        let limit = config.tile_cache_mb.saturating_mul(1024 * 1024);
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, &write_bytes);
            if limit > 0 && WRITES.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY == 0 {
                prune(&cache_root, limit);
            }
        });
    }
    Ok(tile_response(bytes))
}

async fn fetch_tile(url: &str, provider: &str) -> anyhow::Result<Vec<u8>> {
    let mut request = crate::tools::shared_http_client()
        .get(url)
        .timeout(TILE_TIMEOUT);
    if provider == "osm" {
        request = request.header(reqwest::header::USER_AGENT, OSM_USER_AGENT);
    }
    let response = request.send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_TILE_BYTES {
        anyhow::bail!("tile too large");
    }
    if bytes.is_empty() {
        anyhow::bail!("empty tile");
    }
    Ok(bytes.to_vec())
}

fn tile_response(bytes: Vec<u8>) -> Response {
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    // 浏览器侧只缓存一小时:磁盘那层才是真正的缓存,这里短一点,改了 TTL
    // 或换了源之后不用等浏览器自己过期。
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_paths_only_ever_hold_numbers() {
        let root = std::path::Path::new("/tmp/cache");
        let path = cache_path(root, "osm", 3, 4, 5);
        assert!(path.ends_with("map-tiles/osm/3/4/5.png"));
    }

    #[test]
    fn unknown_providers_have_no_upstream() {
        assert!(upstream_url("osm", 1, 0, 0).is_some());
        assert!(upstream_url("amap", 1, 0, 0).is_some());
        assert!(upstream_url("../etc", 1, 0, 0).is_none());
    }

    #[test]
    fn amap_shards_are_stable_per_tile() {
        // 同一块瓦片必须每次落同一个域名,否则每换一次域名就是一次重新握手。
        let first = upstream_url("amap", 12, 3415, 1775).unwrap();
        let again = upstream_url("amap", 12, 3415, 1775).unwrap();
        assert_eq!(first, again);
    }
}
