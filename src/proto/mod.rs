//! Protobuf 消息定义
//!
//! 手动定义，避免 protoc 编译依赖

use prost::Message;
use serde::{Deserialize, Serialize};

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
    #[prost(string, tag = "7")]
    pub upby: String,
    #[prost(string, tag = "8")]
    pub uptime: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct testtb {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<testtbItem>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SyncRequest {
    #[prost(string, tag = "1")]
    pub table_name: String,
    #[prost(string, tag = "2")]
    pub cid: String,
    #[prost(int32, tag = "3")]
    pub getstart: i32,
    #[prost(int32, tag = "4")]
    pub getnumber: i32,
    #[prost(string, tag = "5")]
    pub last_uptime: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SyncResponse {
    #[prost(int32, tag = "1")]
    pub res: i32,
    #[prost(string, tag = "2")]
    pub errmsg: String,
    #[prost(message, repeated, tag = "3")]
    pub items: Vec<testtbItem>,
    #[prost(int32, tag = "4")]
    pub total: i32,
}
