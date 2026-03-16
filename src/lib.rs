//! axum78 - 基于 axum 的快速 Web API 框架
//!
//! 参考 koa78-base78 的 4 级路由架构: /:apisys/:apimicro/:apiobj/:apifun
//!
//! 请求格式: UpInfo (JSON)
//! 响应格式: Response (JSON)

pub mod context;
pub mod response;
pub mod base_api;
pub mod router;
pub mod server;
pub mod proto;
pub mod sync;
pub mod apigame;

pub use context::{UpInfo, RequestBody, Context};
pub use response::{ApiResponse, ApiError};
pub use base_api::{BaseApi, TableConfig};
pub use router::{ApiRouter, ApiRouter78, Controller78};
pub use server::Server;

pub use async_trait::async_trait;

pub use axum::{
    Router,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use axum::{
    body::Bytes,
    extract::Path as AxumPath,
    http::{header, StatusCode as AxumStatusCode},
    response::IntoResponse,
    routing::any,
    Router as AxumRouter,
};
use std::sync::Arc;
use base::Response;
use database::Sqlite78;

pub struct AppState {
    pub db: Sqlite78,
}

impl AppState {
    pub fn new(db_path: &str) -> Self {
        let mut db = Sqlite78::with_config(db_path, false, false);
        let _ = db.initialize();
        Self { db }
    }
}

pub fn create_router(db_path: &str) -> AxumRouter {
    let state = Arc::new(AppState::new(db_path));
    
    AxumRouter::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
        .route("/health", axum::routing::get(health_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    (AxumStatusCode::OK, "OK")
}

async fn api_handler(
    AxumPath((apisys, apimicro, apiobj, apifun)): AxumPath<(String, String, String, String)>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let apisys_lower = apisys.to_lowercase();
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    if apifun.starts_with('_') || !apisys_lower.starts_with("api") || apimicro_lower.starts_with("dll") {
        let resp = Response::fail("Access denied", 403);
        return (AxumStatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let up: base::UpInfo = match serde_json::from_slice(&body) {
        Ok(u) => u,
        Err(e) => {
            let resp = Response::fail(&format!("解析请求失败: {}", e), -1);
            return (AxumStatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let (status, resp_bytes) = match (apisys_lower.as_str(), apimicro_lower.as_str(), apiobj.as_str()) {
        ("apitest", "testmenu", "testtb") => {
            apitest::testmenu::testtb::handle(&apifun_lower, up, &state.db).await
        }
        ("apisvc", "backsvc", "synclog") => {
            apisvc::backsvc::synclog::handle(&apifun_lower, up, &state.db).await
        }
        _ => {
            let resp = Response::fail(&format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun), 404);
            (AxumStatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    };

    (status, [(header::CONTENT_TYPE, "application/json")], resp_bytes)
}
