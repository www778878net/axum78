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
use async_trait::async_trait;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use prost::Message;

use crate::{Controller78, UpInfo};

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

/// 游戏状态控制器
pub struct GameStateController {
    data: Arc<GameStateData>,
}

impl GameStateController {
    pub fn new() -> Self {
        Self {
            data: Arc::new(GameStateData::new()),
        }
    }
    
    pub fn with_data(data: Arc<GameStateData>) -> Self {
        Self { data }
    }
}

impl Default for GameStateController {
    fn default() -> Self {
        Self::new()
    }
}

/// 领主信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LordState {
    pub level: u64,
    pub exp: u64,
    pub exp_needed: u64,
    pub hero_bonus: u64,
    pub quest_bonus: u64,
    pub resource_bonus: u64,
}

/// AGENT 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub level: u64,
    pub exp: u64,
    pub exp_needed: u64,
    pub wuli: u64,
    pub zhili: u64,
    pub zhengzhi: u64,
    pub meili: u64,
    pub tili: u64,
}

/// 国家信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryState {
    pub name: String,
    pub population: u64,
    pub army: u64,
    pub treasury: u64,
    pub food: u64,
}

/// 静态配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticConfig {
    pub round_duration: u64,
    pub lord_hero_bonus_per_level: u64,
    pub lord_quest_bonus_per_level: u64,
    pub lord_resource_bonus_per_level: u64,
}

#[async_trait]
impl Controller78 for GameStateController {
    async fn call(&self, up: &mut UpInfo, fun: &str) -> Value {
        match fun.to_lowercase().as_str() {
            "getinit" => self.get_init(up).await,
            "getsync" => self.get_sync(up).await,
            "signin" => self.sign_in(up).await,
            _ => {
                up.res = -1;
                up.errmsg = format!("Unknown method: {}", fun);
                Value::Null
            }
        }
    }
}

impl GameStateController {
    /// 获取初始数据
    async fn get_init(&self, _up: &mut UpInfo) -> Value {
        let state = serde_json::json!({
            "round": self.data.round.load(Ordering::Relaxed),
            "countdown": self.data.countdown.load(Ordering::Relaxed),
            "points": self.data.points.load(Ordering::Relaxed),
            "coins": self.data.coins.load(Ordering::Relaxed),
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
        
        serde_json::json!({
            "success": true,
            "data": {
                "state": state,
                "lord": lord,
                "country": country,
                "agents": agents,
                "config": config
            }
        })
    }
    
    /// 轻量同步
    async fn get_sync(&self, _up: &mut UpInfo) -> Value {
        serde_json::json!({
            "success": true,
            "data": {
                "round": self.data.round.load(Ordering::Relaxed),
                "countdown": self.data.countdown.load(Ordering::Relaxed),
                "points": self.data.points.load(Ordering::Relaxed),
                "coins": self.data.coins.load(Ordering::Relaxed)
            }
        })
    }
    
    /// 每日签到
    async fn sign_in(&self, _up: &mut UpInfo) -> Value {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        let points = 50 + (timestamp % 100);
        let coins = 20 + ((timestamp / 100) % 50);
        
        self.data.points.fetch_add(points, Ordering::Relaxed);
        self.data.coins.fetch_add(coins, Ordering::Relaxed);
        
        serde_json::json!({
            "success": true,
            "data": {
                "points": points,
                "coins": coins,
                "total_points": self.data.points.load(Ordering::Relaxed),
                "total_coins": self.data.coins.load(Ordering::Relaxed)
            }
        })
    }
}
