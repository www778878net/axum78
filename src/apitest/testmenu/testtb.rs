//! testtb API实现
//!
//! 路径: apitest/testmenu/testtb
//! 路由: POST /apitest/testmenu/testtb/:apifun
//!
//! 参考 LOGSVC 的 CidBase78 实现
//! API函数不需要参数，通过up访问数据，db从框架获取

use axum::{
    body::Bytes,
    http::StatusCode,
};
use base::{UpInfo, Response};
use database::Sqlite78;
use prost::Message;
use serde::{Deserialize, Serialize};

// ============ Proto定义 ============

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

// ============ API实现 ============

/// 处理testtb API请求
pub async fn handle(apifun: &str, up: UpInfo, db: &Sqlite78) -> (StatusCode, Bytes) {
    match apifun.to_lowercase().as_str() {
        "get" => get(&up, db).await,
        "test" => test(&up).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

 

/// TEST - 测试接口
async fn test(up: &UpInfo) -> (StatusCode, Bytes) {
    let resp = Response::success_json(&serde_json::json!({
        "message": "testtb test ok",
        "sid": up.sid
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

fn ensure_testtb_table(db: &Sqlite78) {
    let up = UpInfo::new();
    let sql = r#"CREATE TABLE IF NOT EXISTS testtb (
        idpk INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL UNIQUE,
        cid TEXT NOT NULL DEFAULT '',
        kind TEXT NOT NULL DEFAULT '',
        item TEXT NOT NULL DEFAULT '',
        data TEXT NOT NULL DEFAULT '',
        upby TEXT NOT NULL DEFAULT '',
        uptime TEXT NOT NULL DEFAULT ''
    )"#;
    let _ = db.do_m(sql, &[], &up);
}
