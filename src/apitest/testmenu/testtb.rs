//! testtb - MySQL 通用只读 Controller 示例
//!
//! 路径: apitest/testmenu/testtb
//! 路由: POST /apitest/testmenu/testtb/:apifun
//!
//! 使用 axum78 基框架的 MysqlCidBase78，get 方法自动拼 SQL，
//! 不需要手写 SQL。对应 NodeJS：
//!   class testtb extends CidBase78 {}  // 空类就有全部功能
//!
//! 每个 API 一个独立函数（health、get、test），和 logsvc 一样。

use axum::{
    body::Bytes,
    http::{Method, StatusCode},
};
use base::{UpInfo, Response};
use datastate::{Mysql78, MysqlConfig};
use crate::VerifyResult;
use crate::router::Controller78;
use crate::MysqlCidBase78;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

// ============ MySQL 连接池 ============

static MYSQL_POOL: Lazy<Arc<Mutex<Option<Arc<Mysql78>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

fn get_mysql_config() -> MysqlConfig {
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
                        host, port, user, password, database,
                        max_connections: mysql_section.get("max_connections").and_then(|s| s.parse().ok()).unwrap_or(10),
                        is_log: mysql_section.get("is_log").and_then(|s| s.parse().ok()).unwrap_or(false),
                        is_count: mysql_section.get("is_count").and_then(|s| s.parse().ok()).unwrap_or(false),
                    };
                }
            }
        }
    }
    MysqlConfig {
        host: "127.0.0.1".to_string(), port: 3306,
        user: "root".to_string(), password: String::new(),
        database: "testdb".to_string(), max_connections: 10,
        is_log: false, is_count: false,
    }
}

fn get_mysql_connection() -> Result<Arc<Mysql78>, String> {
    let pool = MYSQL_POOL.clone();
    let mut pool_guard = pool.lock().map_err(|e| format!("获取连接池锁失败: {}", e))?;
    if pool_guard.is_none() {
        let config = get_mysql_config();
        let mut mysql = Mysql78::new(config);
        mysql.initialize()?;
        *pool_guard = Some(Arc::new(mysql));
    }
    Ok(pool_guard.as_ref().unwrap().clone())
}

// ============ API 处理器（每个 API 一个独立函数）============

async fn health() -> (StatusCode, Bytes) {
    let resp = Response::success_json(&serde_json::json!({"status": "OK"}));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

async fn test(up: &UpInfo) -> (StatusCode, Bytes) {
    let resp = Response::success_json(&serde_json::json!({
        "message": "testtb test ok",
        "sid": up.sid
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// get - 通用只读查询（基类自动拼 SQL）
async fn get(up: &UpInfo, base: &MysqlCidBase78) -> (StatusCode, Bytes) {
    match base.get(up).await {
        Ok(rows) => {
            let arr: Vec<Value> = rows.iter()
                .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                .collect();
            let resp = Response::success_json(&Value::Array(arr));
            (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
        Err(e) => {
            let resp = Response::fail(&e, -1);
            (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// handle - 分发到各 API 函数
pub fn handle(apifun: &str, up: UpInfo, base: &MysqlCidBase78) -> (StatusCode, Bytes) {
    match apifun.to_lowercase().as_str() {
        "health" => {
            let (status, bytes) = tokio::runtime::Handle::current().block_on(health());
            (status, bytes)
        }
        "get" => {
            let (status, bytes) = tokio::runtime::Handle::current().block_on(get(&up, base));
            (status, bytes)
        }
        "test" => {
            let (status, bytes) = tokio::runtime::Handle::current().block_on(test(&up));
            (status, bytes)
        }
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

// ============ Controller78 实现 ============

/// testtb 控制器 —— 对应 NodeJS: class testtb extends CidBase78 {}
pub struct TesttbController {
    base: MysqlCidBase78,
}

impl TesttbController {
    pub fn new() -> Self {
        let mysql = get_mysql_connection().expect("MySQL 连接失败");
        Self {
            base: MysqlCidBase78::new("testtb", mysql),
        }
    }
}

#[async_trait]
impl Controller78 for TesttbController {
    async fn call(&self, up: &mut crate::UpInfo, fun: &str, _method: &Method) -> Value {
        let up_clone = up.clone();
        let (status, bytes) = handle(fun, up_clone, &self.base);
        let resp: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        if status != StatusCode::OK {
            up.res = -1;
            up.errmsg = resp.get("errmsg").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return Value::Null;
        }

        resp.get("back").and_then(|v| {
            if let Some(s) = v.as_str() {
                serde_json::from_str(s).ok()
            } else {
                Some(v.clone())
            }
        }).unwrap_or(Value::Null)
    }
}

/// 注册到全局路由表
pub fn register_controller() {
    crate::router::registry::register("apitest/testmenu/testtb", Arc::new(TesttbController::new()));
}
