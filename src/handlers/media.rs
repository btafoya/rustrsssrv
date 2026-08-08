use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

use crate::errors::Result;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/media/{hash}",
    params(("hash" = String, Path, description = "BLAKE3 content hash")),
    responses(
        (status = 200, description = "Media asset"),
        (status = 404, description = "Media not found")
    )
)]
pub async fn get_media(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse> {
    match state.media.get_by_hash(&hash).await? {
        Some((mime, data)) => {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                mime.parse().unwrap_or_else(|_| {
                    header::HeaderValue::from_static("application/octet-stream")
                }),
            );
            Ok((StatusCode::OK, headers, Bytes::from(data)))
        }
        None => Ok((
            StatusCode::NOT_FOUND,
            axum::http::HeaderMap::new(),
            Bytes::new(),
        )),
    }
}
