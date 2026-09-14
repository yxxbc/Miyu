//! 图库面板:清单 / 图片直出 / 上传 / 改名描述标签 / 删除。
//!
//! 与表情包面板的差别是**故意的**:图库不抄视觉识图、语义检索、内置层与使用
//! 计数那一整套重装备(见 `docs/plan/2026-09-13-gqy-personalization.md` §7)。
//! 这里只是一个「存得住、找得到」的相册。
//!
//! 库目录由登录者自己的配置算出来(`album_root`):管理员按当前人格,成员按
//! 自己家目录下的私有人格。**面板从不接受调用方传路径**——id 在索引里解析,
//! 越界的 id 只会 404。

use crate::tools::album::{
    album_root, dashboard_add, dashboard_delete, dashboard_image, dashboard_list, dashboard_update,
    DashboardPatch, DashboardUpload,
};
use crate::web::*;

/// 单张上传硬顶。图库收的是普通照片,24MiB 与表情包同口径。
pub(in crate::web) const ALBUM_UPLOAD_LIMIT: usize = 24 * 1024 * 1024;

#[derive(Deserialize)]
pub(in crate::web) struct AlbumImageQuery {
    id: String,
}

#[derive(Deserialize)]
pub(in crate::web) struct AlbumUploadQuery {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    filename: String,
}

#[derive(Deserialize)]
pub(in crate::web) struct AlbumDeleteQuery {
    #[serde(default)]
    hard: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::web) struct AlbumPatchBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

/// id 是 8 位十六进制(`store::new_id`)。先在这里挡住,免得奇形怪状的东西
/// 一路走到文件名拼接那层。
fn valid_id(id: &str) -> std::result::Result<String, ApiError> {
    let id = id.trim();
    if id.len() < 4 || id.len() > 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid album id"));
    }
    Ok(id.to_string())
}

fn user_error(error: anyhow::Error) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, safe_error_message(&error))
}

fn root_for(
    state: &DaemonState,
    identity: &WebIdentity,
) -> std::result::Result<std::path::PathBuf, ApiError> {
    let config = super::dash_config_for(state, identity, "")?;
    Ok(album_root(&config, &state.paths))
}

fn split_tags(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == ',' || c == '，' || c == '、')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

pub(in crate::web) async fn dash_album_items(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let identity = require_identity(&headers, &state)?;
    let root = root_for(&state, &identity)?;
    let result = tokio::task::spawn_blocking(move || dashboard_list(&root))
        .await
        .map_err(ApiError::internal)?
        .map_err(|error| ApiError::internal(safe_error_message(&error)))?;
    Ok(Json(result))
}

pub(in crate::web) async fn dash_album_image(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<AlbumImageQuery>,
) -> std::result::Result<Response, ApiError> {
    let identity = require_identity(&headers, &state)?;
    let root = root_for(&state, &identity)?;
    let id = valid_id(&query.id)?;
    let resolved = tokio::task::spawn_blocking(move || dashboard_image(&root, &id))
        .await
        .map_err(ApiError::internal)?;
    let Some((path, mime)) = resolved else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "album entry not found",
        ));
    };
    // 图库里的图可以被面板改名改描述,但**文件本身不会被改写**(改图等于换一条),
    // 所以按 id 缓存是安全的。
    stream_file_response(&path, mime, &headers, None, "private, max-age=86400").await
}

pub(in crate::web) async fn dash_album_upload(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<AlbumUploadQuery>,
    body: Bytes,
) -> std::result::Result<Json<Value>, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    if body.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "empty image"));
    }
    let root = root_for(&state, &identity)?;
    let upload = DashboardUpload {
        name: query.name.trim().to_string(),
        description: query.description.trim().to_string(),
        tags: split_tags(&query.tags),
        filename: query.filename.trim().to_string(),
    };
    let result = tokio::task::spawn_blocking(move || dashboard_add(&root, upload, &body))
        .await
        .map_err(ApiError::internal)?
        .map_err(user_error)?;
    Ok(Json(result))
}

pub(in crate::web) async fn dash_album_patch(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AlbumPatchBody>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let id = valid_id(&id)?;
    let root = root_for(&state, &identity)?;
    let patch = DashboardPatch {
        name: body.name,
        description: body.description,
        tags: body.tags,
    };
    let result = tokio::task::spawn_blocking(move || dashboard_update(&root, &id, patch))
        .await
        .map_err(ApiError::internal)?
        .map_err(user_error)?;
    Ok(Json(result))
}

pub(in crate::web) async fn dash_album_delete(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<AlbumDeleteQuery>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let id = valid_id(&id)?;
    let root = root_for(&state, &identity)?;
    let hard = query.hard;
    let result = tokio::task::spawn_blocking(move || dashboard_delete(&root, &id, hard))
        .await
        .map_err(ApiError::internal)?
        .map_err(user_error)?;
    Ok(Json(result))
}
