//! testtb API实现
//!
//! 路径: apitest/testmenu/testtb
//! 路由: POST /apitest/testmenu/testtb/:apifun
//!
//! 使用 MySQL 数据库

use axum::{
    body::Bytes,
    http::StatusCode,
};
use base::{UpInfo, Response, ProjectPath};
use database::{Mysql78, MysqlConfig, MysqlUpInfo};
use crate::VerifyResult;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

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
    #[prost(string, tag = "7")]
    pub upby: String,
    #[prost(string, tag = "8")]
    pub uptime: String,
}

/// testtb 包含多项的数据结构
#[derive(Clone, PartialEq, Message)]
pub struct testtb {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<testtbItem>,
}

/// MySQL 连接池（全局单例，延迟初始化）
static MYSQL_POOL: Lazy<Arc<Mutex<Option<Arc<Mysql78>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

/// MySQL 配置（从配置文件或环境变量读取）
/// 优先级：配置文件 > 环境变量 > 默认值
fn get_mysql_config() -> MysqlConfig {
    // 优先使用 MYSQL_ 前缀的环境变量
    if let Ok(host) = std::env::var("MYSQL_HOST") {
        return MysqlConfig {
            host,
            port: std::env::var("MYSQL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3306),
            user: std::env::var("MYSQL_USER").unwrap_or_else(|_| "root".to_string()),
            password: std::env::var("MYSQL_PASSWORD").unwrap_or_default(),
            database: std::env::var("MYSQL_DATABASE").unwrap_or_else(|_| "testdb".to_string()),
            max_connections: std::env::var("MYSQL_MAX_CONNECTIONS").ok().and_then(|p| p.parse().ok()).unwrap_or(10),
            is_log: false,
            is_count: false,
        };
    }

    // 从配置文件直接读取（使用 load_ini_config 避免环境变量污染）
    if let Ok(p) = base::ProjectPath::find() {
        if let Ok(ini) = p.load_ini_config() {
            if let Some(mysql_section) = ini.get("mysql") {
                let host = mysql_section.get("host").cloned().unwrap_or_default();
                let port = mysql_section.get("port").and_then(|s| s.parse().ok()).unwrap_or(3306);
                let user = mysql_section.get("user").cloned().unwrap_or_default();
                let password = mysql_section.get("password").cloned().unwrap_or_default();
                let database = mysql_section.get("database").cloned().unwrap_or_default();

                if !host.is_empty() && !user.is_empty() && !database.is_empty() {
                    return MysqlConfig {
                        host,
                        port,
                        user,
                        password,
                        database,
                        max_connections: mysql_section.get("max_connections").and_then(|s| s.parse().ok()).unwrap_or(10),
                        is_log: mysql_section.get("is_log").and_then(|s| s.parse().ok()).unwrap_or(false),
                        is_count: mysql_section.get("is_count").and_then(|s| s.parse().ok()).unwrap_or(false),
                    };
                }
            }
        }
    }

    // 默认配置
    MysqlConfig {
        host: "127.0.0.1".to_string(),
        port: 3306,
        user: "root".to_string(),
        password: String::new(),
        database: "testdb".to_string(),
        max_connections: 10,
        is_log: false,
        is_count: false,
    }
}

/// 获取 MySQL 连接（延迟初始化）
fn get_mysql_connection() -> Result<Arc<Mysql78>, String> {
    let pool = MYSQL_POOL.clone();
    let mut pool_guard = pool.lock().map_err(|e| format!("获取连接池锁失败: {}", e))?;

    if pool_guard.is_none() {
        let config = get_mysql_config();
        eprintln!("DEBUG get_mysql_connection - config: host={}, port={}, user={}, database={}",
            config.host, config.port, config.user, config.database);
        let mut mysql = Mysql78::new(config);
        mysql.initialize()?;
        *pool_guard = Some(Arc::new(mysql));
    }

    Ok(pool_guard.as_ref().unwrap().clone())
}

// ============ API实现 ============

/// 处理testtb API请求
///
/// verify_result: 中间件已经验证过的结果，包含 cid、uid、uname
pub async fn handle(apifun: &str, up: UpInfo, verify_result: &VerifyResult) -> (StatusCode, Bytes) {
    // 从 VerifyResult 获取验证后的 cid/uid（由中间件从数据库验证 SID 后填充）
    let user_cid = verify_result.cid.clone();
    let user_uid = verify_result.uid.clone();

    // SAAS 权限验证：所有用户平等，user_cid 为空则拒绝访问
    if user_cid.is_empty() {
        let resp = Response::fail("未登录或SID无效", -1);
        return (StatusCode::UNAUTHORIZED, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let mysql = match get_mysql_connection() {
        Ok(m) => m,
        Err(e) => {
            let resp = Response::fail(&format!("数据库连接失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    match apifun.to_lowercase().as_str() {
        "health" => health().await,
        "get" => get(&up, &mysql, &user_cid).await,
        "test" => test(&up).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// HEALTH - 健康检查
async fn health() -> (StatusCode, Bytes) {
    let resp = Response::success_json(&serde_json::json!({
        "status": "OK"
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
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
async fn get(up: &UpInfo, mysql: &Mysql78, expected_cid: &str) -> (StatusCode, Bytes) {
    let limit = up.getnumber as i32;

    // 查询 testtb 表，返回所有数据（按原顺序）
    let sql = "SELECT * FROM testtb ORDER BY idpk DESC LIMIT ?";
    let params: Vec<serde_json::Value> = vec![
        serde_json::Value::Number(limit.into())
    ];
    let up_info = MysqlUpInfo::new();

    let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match mysql.do_get(sql, params, &up_info) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    // 过滤数据：只返回属于当前 cid 或 cid 为空的数据
    let items: Vec<testtbItem> = rows.iter().filter_map(|row: &std::collections::HashMap<String, serde_json::Value>| {
        let cid = row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // 只返回当前帐套或 cid 为空的数据
        if cid.is_empty() || cid == expected_cid {
            let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let idpk = row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let kind = row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let item = row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let data = row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let upby = row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let uptime = row.get("uptime").and_then(|v| v.as_str()).unwrap_or("").to_string();

            Some(testtbItem {
                id,
                idpk,
                cid,
                kind,
                item,
                data,
                upby,
                uptime,
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
