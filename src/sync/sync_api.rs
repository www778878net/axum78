//! 同步 API 路由
//!
//! 提供 protobuf 格式的同步端点
//!
//! SID验证流程：
//! 1. 客户端上传时携带SID
//! 2. 服务器验证SID对应的CID与数据中的CID是否一致
//! 3. 不一致则拒绝

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
use database::LocalDB;

use crate::proto::{testtb, testtbItem, SyncRequest, SyncResponse, UploadResponse, SyncError};
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

/// 从SID获取对应的CID
/// 这里简化处理：SID就是CID（实际项目中应该查询数据库或缓存）
fn get_cid_from_sid(sid: &str) -> Option<String> {
    if sid.is_empty() {
        return None;
    }
    // 实际项目中应该查询SID表获取CID
    // 这里简化：SID格式为 "cid|其他信息" 或直接就是CID
    if sid.contains('|') {
        Some(sid.split('|').next().unwrap_or("").to_string())
    } else {
        Some(sid.to_string())
    }
}

async fn upload_handler(
    AxumPath(_table): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let request = match testtb::decode(&*body) {
        Ok(msg) => msg,
        Err(e) => {
            let resp = UploadResponse {
                res: -1,
                errmsg: format!("解码失败: {}", e),
                total: 0,
                errors: vec![],
            };
            return (StatusCode::BAD_REQUEST, resp.encode_to_vec()).into_response();
        }
    };

    let sid = request.sid;
    let items = request.items;

    // 验证SID
    let expected_cid = match get_cid_from_sid(&sid) {
        Some(cid) => cid,
        None => {
            let resp = UploadResponse {
                res: -1,
                errmsg: "无效的SID".to_string(),
                total: 0,
                errors: vec![],
            };
            return (StatusCode::UNAUTHORIZED, resp.encode_to_vec()).into_response();
        }
    };

    let sync = DataSync::with_remote_db(&state.db_path);
    sync.ensure_table().expect("建表失败");

    let mut inserted = 0i32;
    let mut updated = 0i32;
    let mut skipped = 0i32;
    let mut errors: Vec<SyncError> = Vec::new();

    for (index, item) in items.iter().enumerate() {
        // 验证CID
        if !item.cid.is_empty() && item.cid != expected_cid {
            errors.push(SyncError {
                index: index as i32,
                idrow: item.id.clone(),
                error: format!("cid 不匹配，期望 {}，实际 {}", expected_cid, item.cid),
            });
            continue;
        }

        match sync.apply_remote_update(item) {
            Ok(s) if s == "inserted" => inserted += 1,
            Ok(s) if s == "updated" => updated += 1,
            Ok(s) if s == "skipped" => skipped += 1,
            Ok(_) => {}
            Err(e) => {
                errors.push(SyncError {
                    index: index as i32,
                    idrow: item.id.clone(),
                    error: e,
                });
            }
        }
    }

    let resp = UploadResponse {
        res: if errors.is_empty() { 0 } else { 1 },
        errmsg: if errors.is_empty() { String::new() } else { format!("{} 条记录验证失败", errors.len()) },
        total: inserted + updated + skipped,
        errors,
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
                cid: String::new(),
            };
            return (StatusCode::BAD_REQUEST, resp.encode_to_vec()).into_response();
        }
    };

    // 验证SID
    let expected_cid = match get_cid_from_sid(&request.sid) {
        Some(cid) => cid,
        None => {
            let resp = SyncResponse {
                res: -1,
                errmsg: "无效的SID".to_string(),
                items: vec![],
                total: 0,
                cid: String::new(),
            };
            return (StatusCode::UNAUTHORIZED, resp.encode_to_vec()).into_response();
        }
    };

    let mut sync = DataSync::with_remote_db(&state.db_path);
    sync.ensure_table().expect("建表失败");
    sync.config.cid = request.cid.clone();
    sync.config.getnumber = request.getnumber;

    let items = match sync.get_items() {
        Ok(items) => items,
        Err(e) => {
            let resp = SyncResponse {
                res: -1,
                errmsg: format!("查询失败: {}", e),
                items: vec![],
                total: 0,
                cid: expected_cid,
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
        cid: expected_cid,
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
                cid: String::new(),
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
        cid: String::new(),
    };

    (StatusCode::OK, resp.encode_to_vec()).into_response()
}
