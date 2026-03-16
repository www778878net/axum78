//! axum78 同步服务器 - 重构版
//!
//! 参考 LOGSVC 实现4级路由
//! 路由格式: /:apisys/:apimicro/:apiobj/:apifun
//! 数据格式: Protobuf
//!
//! 示例:
//! - POST /apitest/testmenu/testtb/get
//! - POST /apitest/testmenu/testtb/mAddMany

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{any, get, post},
    Router,
};
use prost::Message;
use std::sync::Arc;

use crate::proto::{SyncRequest, SyncResponse, UploadRequest, UploadResponse, SyncError, testtbItem};

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
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn api_handler(
    Path((apisys, apimicro, apiobj, apifun)): Path<(String, String, String, String)>,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    let apisys_lower = apisys.to_lowercase();
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    if apifun.starts_with('_') || !apisys_lower.starts_with("api") || apimicro_lower.starts_with("dll") {
        let response = SyncResponse {
            res: 403,
            errmsg: "Access denied".to_string(),
            items: vec![],
        };
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "application/x-protobuf")], Bytes::from(response.encode_to_vec()));
    }

    let response = match (apisys_lower.as_str(), apimicro_lower.as_str(), apiobj.as_str(), apifun_lower.as_str()) {
        ("apitest", "testmenu", "test78", "test") => {
            SyncResponse {
                res: 0,
                errmsg: "看到我说明路由ok,中文ok,无权限调用OK".to_string(),
                items: vec![],
            }
        }
        ("apitest", "testmenu", "testtb", "get") => {
            handle_testtb_get(&state, &body).await
        }
        ("apitest", "testmenu", "testtb", "maddmany") => {
            let upload_response = handle_testtb_maddmany(&state, &body).await;
            return (StatusCode::OK, [(header::CONTENT_TYPE, "application/x-protobuf")], Bytes::from(upload_response.encode_to_vec()));
        }
        _ => {
            SyncResponse {
                res: 404,
                errmsg: format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun),
                items: vec![],
            }
        }
    };

    let status = if response.res == 0 {
        StatusCode::OK
    } else if response.res == 403 {
        StatusCode::FORBIDDEN
    } else if response.res == 404 {
        StatusCode::NOT_FOUND
    } else if response.res == -1 {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    };

    (status, [(header::CONTENT_TYPE, "application/x-protobuf")], Bytes::from(response.encode_to_vec()))
}

async fn handle_testtb_get(state: &AppState, body: &[u8]) -> SyncResponse {
    use crate::sync::DataSync;
    
    let sync = DataSync::with_remote_db(&state.db_path);
    if let Err(e) = sync.ensure_table() {
        return SyncResponse {
            res: -1,
            errmsg: format!("建表失败: {}", e),
            items: vec![],
        };
    }

    let request = match SyncRequest::decode(body) {
        Ok(r) => r,
        Err(e) => {
            return SyncResponse {
                res: -1,
                errmsg: format!("解析请求失败: {}", e),
                items: vec![],
            };
        }
    };

    let expected_cid = if request.sid.is_empty() {
        return SyncResponse {
            res: -1,
            errmsg: "无效的SID".to_string(),
            items: vec![],
        };
    } else if request.sid.contains('|') {
        request.sid.split('|').next().unwrap_or("").to_string()
    } else {
        request.sid.clone()
    };

    let items = match sync.get_items() {
        Ok(items) => items,
        Err(e) => {
            return SyncResponse {
                res: -1,
                errmsg: format!("查询失败: {}", e),
                items: vec![],
            };
        }
    };

    let filtered: Vec<_> = items.into_iter()
        .filter(|item| item.cid == expected_cid || item.cid.is_empty())
        .collect();

    SyncResponse {
        res: 0,
        errmsg: "ok".to_string(),
        items: filtered,
    }
}

async fn handle_testtb_maddmany(state: &AppState, body: &[u8]) -> UploadResponse {
    use crate::sync::DataSync;
    
    let sync = DataSync::with_remote_db(&state.db_path);
    if let Err(e) = sync.ensure_table() {
        return UploadResponse {
            res: -1,
            errmsg: format!("建表失败: {}", e),
            total: 0,
            errors: vec![],
        };
    }

    let request = match UploadRequest::decode(body) {
        Ok(r) => r,
        Err(e) => {
            return UploadResponse {
                res: -1,
                errmsg: format!("解析请求失败: {}", e),
                total: 0,
                errors: vec![],
            };
        }
    };

    let expected_cid = if request.sid.is_empty() {
        return UploadResponse {
            res: -1,
            errmsg: "无效的SID".to_string(),
            total: 0,
            errors: vec![],
        };
    } else if request.sid.contains('|') {
        request.sid.split('|').next().unwrap_or("").to_string()
    } else {
        request.sid.clone()
    };

    let mut total = 0i32;
    let mut errors: Vec<SyncError> = Vec::new();

    for (i, item) in request.items.iter().enumerate() {
        if !item.cid.is_empty() && item.cid != expected_cid {
            errors.push(SyncError {
                index: i as i32,
                idrow: item.id.clone(),
                error: format!("cid 不匹配，期望 {}，实际 {}", expected_cid, item.cid),
            });
            continue;
        }

        let new_item = testtbItem {
            id: if item.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { item.id.clone() },
            idpk: 0,
            cid: expected_cid.clone(),
            kind: item.kind.clone(),
            item: item.item.clone(),
            data: item.data.clone(),
        };

        match sync.apply_remote_update(&new_item) {
            Ok(_) => total += 1,
            Err(e) => {
                errors.push(SyncError {
                    index: i as i32,
                    idrow: new_item.id,
                    error: e,
                });
            }
        }
    }

    UploadResponse {
        res: 0,
        errmsg: "ok".to_string(),
        total,
        errors,
    }
}
