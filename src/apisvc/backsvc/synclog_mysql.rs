//! synclog_mysql API实现 - MySQL 版本
//!
//! 路径: apisvc/backsvc/synclog_mysql
//! 路由: POST /apisvc/backsvc/synclog_mysql/:apifun
//!
//! 核心功能：
//! - m_add_many: 批量添加 synclog 并执行 SQL，返回成功/失败 ID 列表
//! - get: 获取待同步记录
//!
//! 权限控制：
//! - 用户只能操作自己帐套(cid)的数据

use axum::{
    body::Bytes,
    http::StatusCode,
};
use base::{UpInfo, Response};
use database::{Mysql78, MysqlConfig};
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// MySQL 连接池（全局单例）
static MYSQL_POOL: once_cell::sync::Lazy<Arc<Mutex<Option<Mysql78>>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// synclog 项（与 SQLite 版本共用结构）
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct SynclogItem {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub apisys: String,
    #[prost(string, tag = "3")]
    pub apimicro: String,
    #[prost(string, tag = "4")]
    pub apiobj: String,
    #[prost(string, tag = "5")]
    pub tbname: String,
    #[prost(string, tag = "6")]
    pub action: String,
    #[prost(string, tag = "7")]
    pub cmdtext: String,
    #[prost(string, tag = "8")]
    pub params: String,
    #[prost(string, tag = "9")]
    pub idrow: String,
    #[prost(string, tag = "10")]
    pub worker: String,
    #[prost(int32, tag = "11")]
    pub synced: i32,
    #[prost(string, tag = "12")]
    pub cmdtextmd5: String,
    #[prost(string, tag = "13")]
    pub cid: String,
    #[prost(string, tag = "14")]
    pub upby: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SynclogBatch {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<SynclogItem>,
}

/// 执行结果
#[derive(Serialize)]
pub struct ExecutionResult {
    pub success_ids: Vec<String>,
    pub failed: Vec<FailedItem>,
}

#[derive(Serialize)]
pub struct FailedItem {
    pub id: String,
    pub idrow: String,
    pub error: String,
}

/// MySQL 配置（从环境变量或配置文件读取）
fn get_mysql_config() -> MysqlConfig {
    MysqlConfig {
        host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("MYSQL_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3306),
        user: std::env::var("MYSQL_USER").unwrap_or_else(|_| "root".to_string()),
        password: std::env::var("MYSQL_PASSWORD").unwrap_or_default(),
        database: std::env::var("MYSQL_DATABASE").unwrap_or_else(|_| "testdb".to_string()),
        max_connections: 10,
        is_log: false,
        is_count: false,
    }
}

/// 获取或初始化 MySQL 连接池
fn get_mysql_connection() -> Result<Mysql78, String> {
    let pool = MYSQL_POOL.clone();
    let mut pool_guard = pool.lock().map_err(|e| format!("获取连接池锁失败: {}", e))?;
    
    if pool_guard.is_none() {
        let config = get_mysql_config();
        let mut mysql = Mysql78::new(config);
        mysql.initialize()?;
        *pool_guard = Some(mysql);
    }
    
    // 返回一个克隆的连接（实际使用连接池内部的连接）
    Ok(pool_guard.as_ref().unwrap().clone())
}

/// 处理 API 请求
pub async fn handle(apifun: &str, up: UpInfo) -> (StatusCode, Bytes) {
    // 提取 cid 进行权限验证
    let expected_cid = extract_cid_from_sid(&up.sid);
    if expected_cid.is_empty() {
        let resp = Response::fail("无效的SID", -1);
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
        "maddmany" => m_add_many(&up, &mysql, &expected_cid).await,
        "get" => get(&up, &mysql, &expected_cid).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// 从 SID 提取 cid
fn extract_cid_from_sid(sid: &str) -> String {
    if sid.is_empty() {
        return String::new();
    }
    if sid.contains('|') {
        sid.split('|').next().unwrap_or("").to_string()
    } else {
        sid.to_string()
    }
}

/// 从 SID 提取 worker
fn extract_worker_from_sid(sid: &str) -> String {
    if sid.contains('|') {
        sid.split('|').nth(1).unwrap_or("").to_string()
    } else {
        String::new()
    }
}

/// 批量添加 synclog 并执行 SQL
async fn m_add_many(up: &UpInfo, mysql: &Mysql78, expected_cid: &str) -> (StatusCode, Bytes) {
    // 解码请求数据
    let batch: SynclogBatch = match decode_batch(up) {
        Ok(b) => b,
        Err(e) => {
            let resp = Response::fail(&e, -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    // 确保 synclog 表存在
    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let mut success_ids: Vec<String> = Vec::new();
    let mut failed: Vec<FailedItem> = Vec::new();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    for item in batch.items {
        let id = if item.id.is_empty() { 
            database::next_id_string() 
        } else { 
            item.id.clone() 
        };

        // 权限检查：item.cid 必须与用户 cid 一致
        if !item.cid.is_empty() && item.cid != expected_cid {
            failed.push(FailedItem {
                id: id.clone(),
                idrow: item.idrow.clone(),
                error: "无权限操作此帐套数据".to_string(),
            });
            continue;
        }

        // 执行 SQL
        let exec_result = execute_synclog_item(mysql, &item, expected_cid, &now);
        
        match exec_result {
            Ok(_) => {
                // 写入 synclog 成功记录
                let _ = insert_synclog(mysql, &item, &id, expected_cid, 1, "", &now);
                success_ids.push(id);
            }
            Err(e) => {
                // 写入 synclog 失败记录
                let _ = insert_synclog(mysql, &item, &id, expected_cid, -1, &e, &now);
                failed.push(FailedItem {
                    id: id.clone(),
                    idrow: item.idrow.clone(),
                    error: e,
                });
            }
        }
    }

    let result = ExecutionResult { success_ids, failed };
    let resp = Response::success_json(&serde_json::to_value(result).unwrap_or_default());
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 解码批量数据
fn decode_batch(up: &UpInfo) -> Result<SynclogBatch, String> {
    if let Some(data) = &up.bytedata {
        SynclogBatch::decode(&**data)
            .map_err(|e| format!("Protobuf解码失败: {}", e))
    } else if let Some(jsdata) = &up.jsdata {
        use base64::{Engine as _, engine::general_purpose};
        let bytes = general_purpose::STANDARD
            .decode(jsdata)
            .map_err(|e| format!("Base64解码失败: {}", e))?;
        SynclogBatch::decode(&*bytes)
            .map_err(|e| format!("Protobuf解码失败: {}", e))
    } else {
        Err("无bytedata或jsdata".to_string())
    }
}

/// 执行单条 synclog SQL
fn execute_synclog_item(
    mysql: &Mysql78, 
    item: &SynclogItem, 
    _cid: &str,
    _now: &str,
) -> Result<(), String> {
    let params: Vec<Value> = serde_json::from_str(&item.params).unwrap_or_default();
    let up = database::MysqlUpInfo::new();

    match item.action.as_str() {
        "insert" => {
            // 直接执行 cmdtext
            let result = mysql.do_m_add(&item.cmdtext, params, &up);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "插入失败".to_string())),
                Err(e) => Err(e),
            }
        }
        "update" => {
            let result = mysql.do_m(&item.cmdtext, params, &up);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "更新失败".to_string())),
                Err(e) => Err(e),
            }
        }
        "delete" => {
            let result = mysql.do_m(&item.cmdtext, params, &up);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "删除失败".to_string())),
                Err(e) => Err(e),
            }
        }
        _ => Err(format!("未知的action: {}", item.action)),
    }
}

/// 插入 synclog 记录
fn insert_synclog(
    mysql: &Mysql78,
    item: &SynclogItem,
    id: &str,
    cid: &str,
    synced: i32,
    lasterrinfo: &str,
    uptime: &str,
) -> Result<(), String> {
    let sql = r#"INSERT INTO synclog 
        (id, apisys, apimicro, apiobj, tbname, action, cmdtext, params, idrow, worker, synced, lasterrinfo, cmdtextmd5, cid, upby, uptime) 
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#;
    
    let params: Vec<Value> = vec![
        Value::String(id.to_string()),
        Value::String(item.apisys.clone()),
        Value::String(item.apimicro.clone()),
        Value::String(item.apiobj.clone()),
        Value::String(item.tbname.clone()),
        Value::String(item.action.clone()),
        Value::String(item.cmdtext.clone()),
        Value::String(item.params.clone()),
        Value::String(item.idrow.clone()),
        Value::String(item.worker.clone()),
        Value::Number(synced.into()),
        Value::String(lasterrinfo.to_string()),
        Value::String(item.cmdtextmd5.clone()),
        Value::String(cid.to_string()),
        Value::String(item.upby.clone()),
        Value::String(uptime.to_string()),
    ];

    let up = database::MysqlUpInfo::new();
    let result = mysql.do_m_add(sql, params, &up);
    match result {
        Ok(r) if r.error.is_none() => Ok(()),
        Ok(r) => Err(r.error.unwrap_or_else(|| "插入synclog失败".to_string())),
        Err(e) => Err(e),
    }
}

/// 获取待同步记录
async fn get(up: &UpInfo, mysql: &Mysql78, expected_cid: &str) -> (StatusCode, Bytes) {
    let expected_worker = extract_worker_from_sid(&up.sid);
    let limit = up.getnumber as i32;

    // 确保表存在
    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    // 查询 synced=1（已同步到服务器）且不是当前 worker 的记录
    let sql = "SELECT * FROM synclog WHERE synced = 1 AND cid = ? AND worker != ? ORDER BY idpk ASC LIMIT ?";
    let params: Vec<Value> = vec![
        Value::String(expected_cid.to_string()),
        Value::String(expected_worker.clone()),
        Value::Number(limit.into()),
    ];

    let up = database::MysqlUpInfo::new();
    let rows = match mysql.do_get(sql, params, &up) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let items: Vec<SynclogItem> = rows.iter().map(|row| SynclogItem {
        id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        apisys: row.get("apisys").and_then(|v| v.as_str()).unwrap_or("v1").to_string(),
        apimicro: row.get("apimicro").and_then(|v| v.as_str()).unwrap_or("iflow").to_string(),
        apiobj: row.get("apiobj").and_then(|v| v.as_str()).unwrap_or("synclog").to_string(),
        tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        cmdtext: row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        params: row.get("params").and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }).unwrap_or("[]").to_string(),
        idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        synced: row.get("synced").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        cmdtextmd5: row.get("cmdtextmd5").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }).collect();

    let batch = SynclogBatch { items };
    let bytedata = batch.encode_to_vec();
    use base64::{Engine as _, engine::general_purpose};
    let bytedata_base64 = general_purpose::STANDARD.encode(&bytedata);
    
    let resp = Response::success_json(&serde_json::json!({ "bytedata": bytedata_base64 }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 确保 synclog 表存在
fn ensure_synclog_table(mysql: &Mysql78) -> Result<(), String> {
    let sql = r#"CREATE TABLE IF NOT EXISTS synclog (
        idpk INT AUTO_INCREMENT PRIMARY KEY,
        id VARCHAR(64) NOT NULL UNIQUE,
        apisys VARCHAR(50) NOT NULL DEFAULT 'v1',
        apimicro VARCHAR(50) NOT NULL DEFAULT 'iflow',
        apiobj VARCHAR(50) NOT NULL DEFAULT 'synclog',
        tbname VARCHAR(100) NOT NULL DEFAULT '',
        action VARCHAR(20) NOT NULL DEFAULT '',
        cmdtext TEXT NOT NULL,
        params TEXT NOT NULL,
        idrow VARCHAR(64) NOT NULL DEFAULT '',
        worker VARCHAR(100) NOT NULL DEFAULT '',
        synced INT NOT NULL DEFAULT 0,
        lasterrinfo TEXT NOT NULL DEFAULT '',
        cmdtextmd5 VARCHAR(64) NOT NULL DEFAULT '',
        cid VARCHAR(64) NOT NULL DEFAULT '',
        upby VARCHAR(100) NOT NULL DEFAULT '',
        uptime DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        INDEX idx_synced (synced),
        INDEX idx_cid_worker (cid, worker)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#;

    let up = database::MysqlUpInfo::new();
    mysql.do_m(sql, vec![], &up)
        .map(|_| ())
        .map_err(|e| format!("创建synclog表失败: {}", e))
}
