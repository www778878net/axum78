//! 游戏状态 Mock API
//!
//! 路径: apigame/mock/game_state
//!
//! API:
//! - GetInit: 获取初始数据 (包含所有数据)
//! - GetSync: 轻量同步 (只返回变化的数据)
//! - SignIn: 每日签到

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use axum::http::StatusCode;
use axum::body::Bytes;
use base::Response;
use base::UpInfo;

/// 游戏状态共享数据
pub static GAME_STATE: once_cell::sync::Lazy<Arc<GameStateData>> = 
    once_cell::sync::Lazy::new(|| Arc::new(GameStateData::new()));

/// 游戏状态共享数据
pub struct GameStateData {
    pub round: AtomicU64,
    pub countdown: AtomicU64,
    pub points: AtomicU64,
    pub coins: AtomicU64,
}

impl GameStateData {
    pub fn new() -> Self {
        Self {
            round: AtomicU64::new(1),
            countdown: AtomicU64::new(300),
            points: AtomicU64::new(1250),
            coins: AtomicU64::new(5820),
        }
    }
}

impl Default for GameStateData {
    fn default() -> Self {
        Self::new()
    }
}

/// 处理 API 请求
pub async fn handle(fun: &str, up: UpInfo) -> (StatusCode, Bytes) {
    let data = GAME_STATE.clone();
    match fun.to_lowercase().as_str() {
        "getinit" => get_init(&data, &up).await,
        "getsync" => get_sync(&data, &up).await,
        "signin" => sign_in(&data, &up).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", fun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// 获取初始数据
async fn get_init(data: &Arc<GameStateData>, _up: &UpInfo) -> (StatusCode, Bytes) {
    let state = serde_json::json!({
        "round": data.round.load(Ordering::Relaxed),
        "countdown": data.countdown.load(Ordering::Relaxed),
        "points": data.points.load(Ordering::Relaxed),
        "coins": data.coins.load(Ordering::Relaxed),
        "agent_count": 3
    });
    
    let lord = serde_json::json!({
        "level": 5,
        "exp": 7500,
        "exp_needed": 10000,
        "hero_bonus": 15,
        "quest_bonus": 20,
        "resource_bonus": 10
    });
    
    let country = serde_json::json!({
        "name": "测试王国",
        "population": 10000,
        "army": 500,
        "treasury": 5000,
        "food": 8000
    });
    
    let agents = serde_json::json!([
        {
            "id": "agent_1",
            "name": "浪人",
            "level": 3,
            "exp": 150,
            "exp_needed": 500,
            "wuli": 85,
            "zhili": 70,
            "zhengzhi": 60,
            "meili": 75,
            "tili": 100
        },
        {
            "id": "agent_2",
            "name": "商人",
            "level": 2,
            "exp": 80,
            "exp_needed": 300,
            "wuli": 40,
            "zhili": 90,
            "zhengzhi": 85,
            "meili": 80,
            "tili": 100
        },
        {
            "id": "agent_3",
            "name": "工匠",
            "level": 4,
            "exp": 200,
            "exp_needed": 800,
            "wuli": 60,
            "zhili": 75,
            "zhengzhi": 70,
            "meili": 65,
            "tili": 100
        }
    ]);
    
    let config = serde_json::json!({
        "round_duration": 300,
        "lord_hero_bonus_per_level": 3,
        "lord_quest_bonus_per_level": 4,
        "lord_resource_bonus_per_level": 2
    });
    
    let result = serde_json::json!({
        "success": true,
        "data": {
            "state": state,
            "lord": lord,
            "country": country,
            "agents": agents,
            "config": config
        }
    });
    
    let resp = Response::success_json(&result);
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 轻量同步
async fn get_sync(data: &Arc<GameStateData>, _up: &UpInfo) -> (StatusCode, Bytes) {
    let result = serde_json::json!({
        "success": true,
        "data": {
            "round": data.round.load(Ordering::Relaxed),
            "countdown": data.countdown.load(Ordering::Relaxed),
            "points": data.points.load(Ordering::Relaxed),
            "coins": data.coins.load(Ordering::Relaxed)
        }
    });
    
    let resp = Response::success_json(&result);
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 每日签到
async fn sign_in(data: &Arc<GameStateData>, _up: &UpInfo) -> (StatusCode, Bytes) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let points = 50 + (timestamp % 100);
    let coins = 20 + ((timestamp / 100) % 50);
    
    data.points.fetch_add(points, Ordering::Relaxed);
    data.coins.fetch_add(coins, Ordering::Relaxed);
    
    let result = serde_json::json!({
        "success": true,
        "data": {
            "points": points,
            "coins": coins,
            "total_points": data.points.load(Ordering::Relaxed),
            "total_coins": data.coins.load(Ordering::Relaxed)
        }
    });
    
    let resp = Response::success_json(&result);
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}
