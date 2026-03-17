//! testtb API实现
//!
//! 路径: apitest/testmenu/testtb
//! 路由: POST /apitest/testmenu/testtb/:apifun
//!
//! 使用DataState基类访问数据库

use axum::{
    body::Bytes,
    http::StatusCode,
};
use base::{UpInfo, Response};
use database::datastate::TestTb;
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
pub async fn handle(apifun: &str, up: UpInfo) -> (StatusCode, Bytes) {
    match apifun.to_lowercase().as_str() {
        "get" => get(&up).await,
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

/// GET - 获取数据
async fn get(up: &UpInfo) -> (StatusCode, Bytes) {
    // 第一层验证：简单格式验证（从 SID 提取 CID）
    let verify_result = match crate::context::verify_sid_simple(&up.sid) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, Bytes::from(serde_json::to_string(&e).unwrap_or_default()));
        }
    };
    let expected_cid = verify_result.cid;
    
    // 第二层验证：数据库验证（可选，根据配置决定是否启用）
    // 如果需要严格验证，取消下面的注释
    // let lovers_state = crate::get_lovers_state();
    // if let Err(e) = crate::verify_sid_db(&up.sid, &lovers_state) {
    //     return (StatusCode::UNAUTHORIZED, Bytes::from(serde_json::to_string(&e).unwrap_or_default()));
    // }

    // 服务器端使用远程数据库路径
    let remote_db_path = "c:\\7788\\rustdemo\\rustdemo\\crates\\axum78\\tmp\\data\\remote.db";
    let testtb_state = TestTb::with_db_path(remote_db_path);
    
    let rows = match testtb_state.mlist("testtb", up.getnumber as i32, "API查询") {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let items: Vec<testtbItem> = rows.iter().filter_map(|row| {
        if row.cid.is_empty() || row.cid == expected_cid {
            Some(testtbItem {
                id: row.id.clone(),
                idpk: row.idpk as i32,
                cid: row.cid.clone(),
                kind: row.kind.clone(),
                item: row.item.clone(),
                data: row.data.clone(),
            })
        } else {
            None
        }
    }).collect();

    let result = testtb { items };
    let bytedata = result.encode_to_vec();
    let resp = Response::success_bytes(bytedata);
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}
