//! axum78 同步服务器 - 重构版
//!
//! 参考 LOGSVC 实现4级路由
//! 路由格式: /:apisys/:apimicro/:apiobj/:apifun
//! 数据格式: Protobuf
//!
//! 示例:
//! - POST /apitest/testmenu/testtb/get
//! - POST /apitest/testmenu/testtb/mAddMany
//! - POST /apigame/era/game_state/GetState

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::proto::{
    SyncRequest, SyncResponse, UploadRequest, UploadResponse, SyncError, testtbItem,
    GameState, GameStateResponse, LordState, SignInResponse,
};

pub struct AppState {
    pub db_path: String,
    pub game_round: AtomicU64,
    pub game_countdown: AtomicU64,
    pub game_points: AtomicU64,
    pub game_coins: AtomicU64,
}

impl AppState {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
            game_round: AtomicU64::new(1),
            game_countdown: AtomicU64::new(300),
            game_points: AtomicU64::new(1250),
            game_coins: AtomicU64::new(5820),
        }
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

    let (status, response_bytes) = match (apisys_lower.as_str(), apimicro_lower.as_str(), apiobj.as_str(), apifun_lower.as_str()) {
        ("apitest", "testmenu", "test78", "test") => {
            let response = SyncResponse {
                res: 0,
                errmsg: "看到我说明路由ok,中文ok,无权限调用OK".to_string(),
                items: vec![],
            };
            (StatusCode::OK, Bytes::from(response.encode_to_vec()))
        }
        ("apitest", "testmenu", "testtb", "get") => {
            let response = handle_testtb_get(&state, &body).await;
            (StatusCode::OK, Bytes::from(response.encode_to_vec()))
        }
        ("apitest", "testmenu", "testtb", "maddmany") => {
            let response = handle_testtb_maddmany(&state, &body).await;
            (StatusCode::OK, Bytes::from(response.encode_to_vec()))
        }
        ("apigame", "era", "game_state", "getstate") => {
            let response = handle_game_state_get(&state).await;
            (StatusCode::OK, Bytes::from(response.encode_to_vec()))
        }
        ("apigame", "era", "game_state", "signin") => {
            let response = handle_signin(&state).await;
            (StatusCode::OK, Bytes::from(response.encode_to_vec()))
        }
        _ => {
            let response = SyncResponse {
                res: 404,
                errmsg: format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun),
                items: vec![],
            };
            (StatusCode::NOT_FOUND, Bytes::from(response.encode_to_vec()))
        }
    };

    (status, [(header::CONTENT_TYPE, "application/x-protobuf")], response_bytes)
}

async fn handle_testtb_get(state: &Arc<AppState>, body: &[u8]) -> SyncResponse {
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

async fn handle_testtb_maddmany(state: &Arc<AppState>, body: &[u8]) -> UploadResponse {
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

async fn handle_game_state_get(state: &Arc<AppState>) -> GameStateResponse {
    GameStateResponse {
        success: true,
        data: Some(GameState {
            round: state.game_round.load(Ordering::Relaxed),
            countdown: state.game_countdown.load(Ordering::Relaxed),
            points: state.game_points.load(Ordering::Relaxed),
            coins: state.game_coins.load(Ordering::Relaxed),
            agent_count: 3,
            lord: Some(LordState {
                level: 5,
                exp: 7500,
                exp_needed: 10000,
                hero_bonus: 15,
                quest_bonus: 20,
                resource_bonus: 10,
            }),
        }),
        errmsg: "ok".to_string(),
    }
}

async fn handle_signin(state: &Arc<AppState>) -> SignInResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let points = 50 + (timestamp % 100);
    let coins = 20 + ((timestamp / 100) % 50);
    
    state.game_points.fetch_add(points, Ordering::Relaxed);
    state.game_coins.fetch_add(coins, Ordering::Relaxed);
    
    SignInResponse {
        success: true,
        points,
        coins,
        total_points: state.game_points.load(Ordering::Relaxed),
        total_coins: state.game_coins.load(Ordering::Relaxed),
        errmsg: "ok".to_string(),
    }
}
