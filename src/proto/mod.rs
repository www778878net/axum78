//! Protobuf 消息定义
//!
//! 参考 LOGSVC proto/apitest/testmenu/testtb.proto

use prost::Message;
use serde::{Deserialize, Serialize};

/// testtb 单项数据结构
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct testtbItem {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(int32, tag = "2")]
    pub idpk: i32,
    #[prost(string, tag = "3")]
    pub cid: String,
    #[prost(string, tag = "4")]
    pub kind: String,
    #[prost(string, tag = "5")]
    pub item: String,
    #[prost(string, tag = "6")]
    pub data: String,
}

/// testtb 包含多项的数据结构
#[derive(Clone, PartialEq, Message)]
pub struct testtb {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<testtbItem>,
}

/// 同步请求 - 用于 get 操作
#[derive(Clone, PartialEq, Message)]
pub struct SyncRequest {
    #[prost(string, tag = "1")]
    pub sid: String,
    #[prost(string, tag = "2")]
    pub cid: String,
    #[prost(int32, tag = "3")]
    pub getstart: i32,
    #[prost(int32, tag = "4")]
    pub getnumber: i32,
}

/// 同步响应 - 用于 get 操作返回
#[derive(Clone, PartialEq, Message)]
pub struct SyncResponse {
    #[prost(int32, tag = "1")]
    pub res: i32,
    #[prost(string, tag = "2")]
    pub errmsg: String,
    #[prost(message, repeated, tag = "3")]
    pub items: Vec<testtbItem>,
}

/// 上传请求 - 用于 mAddMany 操作
#[derive(Clone, PartialEq, Message)]
pub struct UploadRequest {
    #[prost(string, tag = "1")]
    pub sid: String,
    #[prost(message, repeated, tag = "2")]
    pub items: Vec<testtbItem>,
}

/// 上传响应 - 用于 mAddMany 操作返回
#[derive(Clone, PartialEq, Message)]
pub struct UploadResponse {
    #[prost(int32, tag = "1")]
    pub res: i32,
    #[prost(string, tag = "2")]
    pub errmsg: String,
    #[prost(int32, tag = "3")]
    pub total: i32,
    #[prost(message, repeated, tag = "4")]
    pub errors: Vec<SyncError>,
}

/// 同步错误
#[derive(Clone, PartialEq, Message)]
pub struct SyncError {
    #[prost(int32, tag = "1")]
    pub index: i32,
    #[prost(string, tag = "2")]
    pub idrow: String,
    #[prost(string, tag = "3")]
    pub error: String,
}

/// 游戏状态 - 领主信息
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct LordState {
    #[prost(uint64, tag = "1")]
    pub level: u64,
    #[prost(uint64, tag = "2")]
    pub exp: u64,
    #[prost(uint64, tag = "3")]
    pub exp_needed: u64,
    #[prost(uint64, tag = "4")]
    pub hero_bonus: u64,
    #[prost(uint64, tag = "5")]
    pub quest_bonus: u64,
    #[prost(uint64, tag = "6")]
    pub resource_bonus: u64,
}

/// 游戏状态 - 主结构
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct GameState {
    #[prost(uint64, tag = "1")]
    pub round: u64,
    #[prost(uint64, tag = "2")]
    pub countdown: u64,
    #[prost(uint64, tag = "3")]
    pub points: u64,
    #[prost(uint64, tag = "4")]
    pub coins: u64,
    #[prost(uint64, tag = "5")]
    pub agent_count: u64,
    #[prost(message, optional, tag = "6")]
    pub lord: Option<LordState>,
}

/// 游戏状态响应
#[derive(Clone, PartialEq, Message)]
pub struct GameStateResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(message, optional, tag = "2")]
    pub data: Option<GameState>,
    #[prost(string, tag = "3")]
    pub errmsg: String,
}

/// 签到响应
#[derive(Clone, PartialEq, Message)]
pub struct SignInResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(uint64, tag = "2")]
    pub points: u64,
    #[prost(uint64, tag = "3")]
    pub coins: u64,
    #[prost(uint64, tag = "4")]
    pub total_points: u64,
    #[prost(uint64, tag = "5")]
    pub total_coins: u64,
    #[prost(string, tag = "6")]
    pub errmsg: String,
}
