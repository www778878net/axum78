//! 同步 API 路由
//!
//! 提供 protobuf 格式的同步端点

use axum::{
    body::Bytes,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use prost::Message;
use std::sync::Arc;

use crate::proto::{testtb, SyncRequest, SyncResponse};
use crate::sync::DataSync;

pub struct AppState {
    pub db_path: String,
}

impl AppState {
    pub fn new(db_path: &str) -> Self {
        Self { db_path: db_path.to_string() }
    }
}

pub fn create_router(db_path: &str) -> Router {
    let state = Arc::new(AppState::new(db_path));
    
    Router::new()
        .route("/sync/:table", post(upload_handler))
        .route("/sync/:table/get", post(download_handler))
        .route("/sync/:table/items", get(list_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn upload_handler(
    AxumPath(_table): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let items = match testtb::decode(&*body) {
        Ok(msg) => msg.items,
        Err(e) => {
            let resp = SyncResponse {
                res: -1,
                errmsg: format!("解码失败: {}", e),
                items: vec![],
                total: 0,
            };
            return (StatusCode::BAD_REQUEST, resp.encode_to_vec()).into_response();
        }
    };

    let sync = DataSync::with_remote_db(&state.db_path);
    sync.ensure_table().expect("建表失败");

    let mut inserted = 0i32;
    let mut updated = 0i32;
    let mut skipped = 0i32;

    for item in &items {
        match sync.apply_remote_update(item) {
            Ok(s) if s == "inserted" => inserted += 1,
            Ok(s) if s == "updated" => updated += 1,
            Ok(s) if s == "skipped" => skipped += 1,
            Ok(_) => {}
            Err(e) => {
                let resp = SyncResponse {
                    res: -1,
                    errmsg: format!("处理失败: {}", e),
                    items: vec![],
                    total: inserted + updated,
                };
                return (StatusCode::INTERNAL_SERVER_ERROR, resp.encode_to_vec()).into_response();
            }
        }
    }

    let resp = SyncResponse {
        res: 0,
        errmsg: String::new(),
        items: vec![],
        total: inserted + updated + skipped,
    };

    (StatusCode::OK, resp.encode_to_vec()).into_response()
}

async fn download_handler(
    AxumPath(_table): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let request = match SyncRequest::decode(&*body) {
        Ok(req) => req,
        Err(e) => {
            let resp = SyncResponse {
                res: -1,
                errmsg: format!("解码失败: {}", e),
                items: vec![],
                total: 0,
            };
            return (StatusCode::BAD_REQUEST, resp.encode_to_vec()).into_response();
        }
    };

    let mut sync = DataSync::with_remote_db(&state.db_path);
    sync.ensure_table().expect("建表失败");
    sync.config.cid = request.cid;
    sync.config.getnumber = request.getnumber;

    let items = match sync.get_items() {
        Ok(items) => items,
        Err(e) => {
            let resp = SyncResponse {
                res: -1,
                errmsg: format!("查询失败: {}", e),
                items: vec![],
                total: 0,
            };
            return (StatusCode::INTERNAL_SERVER_ERROR, resp.encode_to_vec()).into_response();
        }
    };

    let total = items.len() as i32;
    let resp = SyncResponse {
        res: 0,
        errmsg: String::new(),
        items,
        total,
    };

    (StatusCode::OK, resp.encode_to_vec()).into_response()
}

async fn list_handler(
    AxumPath(_table): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let sync = DataSync::with_remote_db(&state.db_path);
    sync.ensure_table().expect("建表失败");

    let items = match sync.get_items() {
        Ok(items) => items,
        Err(e) => {
            let resp = SyncResponse {
                res: -1,
                errmsg: format!("查询失败: {}", e),
                items: vec![],
                total: 0,
            };
            return (StatusCode::INTERNAL_SERVER_ERROR, resp.encode_to_vec()).into_response();
        }
    };

    let total = items.len() as i32;
    let resp = SyncResponse {
        res: 0,
        errmsg: String::new(),
        items,
        total,
    };

    (StatusCode::OK, resp.encode_to_vec()).into_response()
}
