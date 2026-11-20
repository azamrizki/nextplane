//! Instance-level endpoints that need no login: health and version.

use crate::extract::Json;
use axum::extract::State;
use axum::http::{Method, Uri};
use np_core::{Error, ErrorKind};
use serde::Serialize;

use crate::error::{ApiError, ApiResult};
use crate::{AppState, VersionInfo};

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    schema_version: Option<i64>,
}

pub async fn health(State(state): State<AppState>) -> ApiResult<Json<Health>> {
    state.store.ping().await?;
    let schema_version = state.store.schema_version().await?;
    Ok(Json(Health {
        status: "ok",
        schema_version,
    }))
}

pub async fn version(State(state): State<AppState>) -> Json<VersionInfo> {
    Json(state.version.clone())
}

pub async fn method_not_allowed(method: Method, uri: Uri) -> ApiError {
    Error::new(
        ErrorKind::MethodNotAllowed,
        format!("{method} is not allowed on {}", uri.path()),
    )
    .into()
}

pub async fn api_not_found(uri: Uri) -> ApiError {
    Error::not_found(format!("no API route for {}", uri.path())).into()
}
