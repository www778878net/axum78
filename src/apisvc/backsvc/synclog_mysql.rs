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
use datastate::{Mysql78, MysqlConfig, MysqlUpInfo};
use prost::Message;
use serde::{Deserialize, Serialize};
use crate::VerifyResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// MySQL 连接池（全局单例，延迟初始化）
static MYSQL_POOL: once_cell::sync::Lazy<Arc<Mutex<Option<Arc<Mysql78>>>>> = 
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
        password: "root123".to_string(),
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

/// 处理 API 请求
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
        "maddmany" => m_add_many(&up, &mysql, &user_cid, &user_uid).await,
        "dowork" => do_work(&up, &mysql, &user_cid).await,
        "get" => get(&up, &mysql, &user_cid).await,
        "getbyworker" => get_by_worker(&up, &mysql, &user_cid).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// 批量添加 synclog（只保存，不执行 SQL）
async fn m_add_many(up: &UpInfo, mysql: &Mysql78, user_cid: &str, _user_uid: &str) -> (StatusCode, Bytes) {
    let batch: SynclogBatch = match decode_batch(up) {
        Ok(b) => b,
        Err(e) => {
            let resp = Response::fail(&e, -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut batches = 0;

    for item in batch.items {
        let id = if item.id.is_empty() {
            datastate::next_id_string()
        } else {
            item.id.clone()
        };

        let sql = r#"INSERT INTO synclog
            (id, apisys, apimicro, apiobj, tbname, action, cmdtext, params, idrow, worker, synced, lasterrinfo, cmdtextmd5, cid, upby, uptime)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, '', ?, ?, ?, ?)"#;

        let params: Vec<Value> = vec![
            Value::String(id),
            Value::String(item.apisys),
            Value::String(item.apimicro),
            Value::String(item.apiobj),
            Value::String(item.tbname),
            Value::String(item.action),
            Value::String(item.cmdtext),
            Value::String(item.params),
            Value::String(item.idrow),
            Value::String(item.worker),
            Value::String(item.cmdtextmd5),
            Value::String(user_cid.to_string()),
            Value::String(item.upby),
            Value::String(now.clone()),
        ];

        let up_info = datastate::MysqlUpInfo::new();
        if let Err(e) = mysql.do_m(sql, params, &up_info) {
            eprintln!("[m_add_many] 插入失败: {}", e);
        } else {
            batches += 1;
        }
    }

    let resp = Response::success_json(&serde_json::json!({ "batches": batches }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 执行待处理的 synclog 记录
async fn do_work(up: &UpInfo, mysql: &Mysql78, _user_cid: &str) -> (StatusCode, Bytes) {
    let data = match up.parse_prefixed_data() {
        Ok(d) => d,
        Err(_) => {
            let resp = Response::fail("数据解析失败", -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let worker = data.get("worker")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if worker.is_empty() {
        let resp = Response::fail("worker参数为空", -1);
        return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let limit = 100;
    let max_batches = 10;
    let mut total_processed = 0i32;
    let mut batch_count = 0i32;

    for _ in 0..max_batches {
        let sql = "SELECT * FROM synclog WHERE synced = 0 AND worker = ? ORDER BY idpk ASC LIMIT ?";
        let params: Vec<Value> = vec![
            Value::String(worker.clone()),
            Value::Number(limit.into()),
        ];

        let up_info = datastate::MysqlUpInfo::new();
        let rows = match mysql.do_get(sql, params, &up_info) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[do_work] 查询失败: {}", e);
                break;
            }
        };

        if rows.is_empty() {
            break;
        }

        batch_count += 1;
        println!("[do_work] 批次 {} 处理 {} 条记录", batch_count, rows.len());

        for row in rows {
            let idpk = row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0);
            let tbname = row.get("tbname").and_then(|v| v.as_str()).unwrap_or("");
            let action = row.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let params_str = match row.get("params") {
                Some(Value::String(s)) => s.clone(),
                Some(v @ Value::Array(_)) => serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()),
                _ => "[]".to_string(),
            };
            let cmdtext = row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("");
            let idrow = row.get("idrow").and_then(|v| v.as_str()).unwrap_or("");
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            let result = execute_synclog_action(mysql, action, cmdtext, &params_str);

            match result {
                Ok(_) => {
                    println!("[do_work] 处理成功: idpk={}", idpk);
                    let update_sql = "UPDATE synclog SET synced = 1, lasterrinfo = '', uptime = ? WHERE idpk = ?";
                    let update_params: Vec<Value> = vec![
                        Value::String(now),
                        Value::Number(idpk.into()),
                    ];
                    let _ = mysql.do_m(update_sql, update_params, &up_info);
                    total_processed += 1;
                }
                Err(e) => {
                    println!("[do_work] 处理失败: idpk={}, error={}", idpk, e);
                    let update_sql = "UPDATE synclog SET synced = -1, lasterrinfo = ?, uptime = ? WHERE idpk = ?";
                    let update_params: Vec<Value> = vec![
                        Value::String(e.clone()),
                        Value::String(now),
                        Value::Number(idpk.into()),
                    ];
                    let _ = mysql.do_m(update_sql, update_params, &up_info);
                }
            }
        }
    }

    let resp = Response::success_json(&serde_json::json!({
        "processed": total_processed,
        "batches": batch_count,
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 执行单条 synclog SQL 操作
fn execute_synclog_action(mysql: &Mysql78, action: &str, cmdtext: &str, params_str: &str) -> Result<(), String> {
    let params: Vec<Value> = serde_json::from_str(params_str).unwrap_or_default();
    let up_info = datastate::MysqlUpInfo::new();

    match action {
        "insert" | "update" | "delete" => {
            let result = mysql.do_m(cmdtext, params, &up_info);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "操作失败".to_string())),
                Err(e) => Err(e),
            }
        }
        _ => Err(format!("未知的action: {}", action)),
    }
}

/// 解码批量数据
fn decode_batch(up: &UpInfo) -> Result<SynclogBatch, String> {
    if let Some(data) = &up.bytedata {
        if data.len() >= 8 {
            let first_byte = data[0];
            let content = &data[8..];
            match first_byte {
                0 => {
                    SynclogBatch::decode(content)
                        .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
                }
                1 => {
                    let json_str = String::from_utf8_lossy(content);
                    let items: Vec<SynclogItem> = serde_json::from_str(&json_str)
                        .map_err(|e| format!("[SYNCLOG_V2] JSON解码失败: {}", e))?;
                    Ok(SynclogBatch { items })
                }
                _ => {
                    SynclogBatch::decode(&**data)
                        .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
                }
            }
        } else {
            SynclogBatch::decode(&**data)
                .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
        }
    } else if let Some(jsdata) = &up.jsdata {
        if jsdata.len() >= 8 {
            let first_char = jsdata.chars().next().unwrap_or(' ');
            let content = &jsdata[8..];
            match first_char {
                '0' => {
                    use base64::{Engine as _, engine::general_purpose};
                    let bytes = general_purpose::STANDARD
                        .decode(content)
                        .map_err(|e| format!("[SYNCLOG_V2] Base64解码失败(0): {}", e))?;
                    SynclogBatch::decode(&*bytes)
                        .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
                }
                '1' => {
                    let items: Vec<SynclogItem> = serde_json::from_str(content)
                        .map_err(|e| format!("[SYNCLOG_V2] JSON解码失败(1): {}, content={}", e, &content[..content.len().min(100)]))?;
                    Ok(SynclogBatch { items })
                }
                _ => {
                    use base64::{Engine as _, engine::general_purpose};
                    let bytes = general_purpose::STANDARD
                        .decode(jsdata)
                        .map_err(|e| format!("[SYNCLOG_V2] Base64解码失败(_): first_char='{}' ({}), jsdata={}", first_char, first_char as u32, &jsdata[..jsdata.len().min(50)]))?;
                    SynclogBatch::decode(&*bytes)
                        .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
                }
            }
        } else {
            use base64::{Engine as _, engine::general_purpose};
            let bytes = general_purpose::STANDARD
                .decode(jsdata)
                .map_err(|e| format!("[SYNCLOG_V2] Base64解码失败(short): {}", e))?;
            SynclogBatch::decode(&*bytes)
                .map_err(|e| format!("[SYNCLOG_V2] Protobuf解码失败: {}", e))
        }
    } else {
        Err("[SYNCLOG_V2] 无bytedata或jsdata".to_string())
    }
}

/// 验证 cid/uid 权限
/// 1. 管理员帐套跳过验证
/// 2. insert 时验证参数中的 cid/uid
/// 3. update/delete 时查表验证
fn validate_cid_uid(
    mysql: &Mysql78,
    item: &SynclogItem,
    user_cid: &str,
    user_uid: &str,
) -> Result<(), String> {
    // insert 时验证参数中的 cid/uid
    if item.action == "insert" {
        let params: Vec<Value> = serde_json::from_str(&item.params).unwrap_or_default();
        
        // 从 cmdtext 解析列名
        if let Some(cols_str) = extract_columns_from_insert(&item.cmdtext) {
            let columns: Vec<&str> = cols_str.split(',').map(|c| c.trim().trim_matches('`')).collect();
            
            for (idx, col) in columns.iter().enumerate() {
                if idx >= params.len() { break; }
                
                if *col == "cid" {
                    if let Some(actual_cid) = params[idx].as_str() {
                        if !actual_cid.is_empty() && actual_cid != user_cid {
                            return Err(format!("cid不匹配，期望{}，实际{}", user_cid, actual_cid));
                        }
                    }
                }
                
                if *col == "uid" {
                    if let Some(actual_uid) = params[idx].as_str() {
                        if !actual_uid.is_empty() && actual_uid != user_uid {
                            return Err(format!("uid不匹配，期望{}，实际{}", user_uid, actual_uid));
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    
    // update/delete 时查表验证
    if (item.action == "update" || item.action == "delete") && !item.idrow.is_empty() {
        let sql = format!("SELECT `cid`, `uid` FROM `{}` WHERE id = ? LIMIT 1", item.tbname);
        let up = datastate::MysqlUpInfo::new();
        let rows = mysql.do_get(&sql, vec![Value::String(item.idrow.clone())], &up);
        
        match rows {
            Ok(r) if !r.is_empty() => {
                let row = &r[0];
                if let Some(actual_cid) = row.get("cid").and_then(|v| v.as_str()) {
                    if !actual_cid.is_empty() && actual_cid != user_cid {
                        return Err(format!("cid不匹配，期望{}，实际{}", user_cid, actual_cid));
                    }
                }
                if let Some(actual_uid) = row.get("uid").and_then(|v| v.as_str()) {
                    if !actual_uid.is_empty() && actual_uid != user_uid {
                        return Err(format!("uid不匹配，期望{}，实际{}", user_uid, actual_uid));
                    }
                }
            }
            _ => {}
        }
    }
    
    Ok(())
}

/// 从 INSERT SQL 中提取列名
fn extract_columns_from_insert(cmdtext: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?i)INSERT\s+INTO\s+`?(\w+)`?\s*\(([^)]+)\)").ok()?;
    let caps = re.captures(cmdtext)?;
    Some(caps[2].to_string())
}

/// 执行单条 synclog SQL
fn execute_synclog_item(
    mysql: &Mysql78, 
    item: &SynclogItem, 
    _cid: &str,
    _now: &str,
) -> Result<(), String> {
    let mut params: Vec<Value> = serde_json::from_str(&item.params).unwrap_or_default();
    let up = datastate::MysqlUpInfo::new();

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
            // update 的 WHERE id = ? 参数需要从 idrow 获取
            // params 已包含 id，直接使用
            let params = params;
            if !item.idrow.is_empty() {
            }
            let result = mysql.do_m(&item.cmdtext, params, &up);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "更新失败".to_string())),
                Err(e) => Err(e),
            }
        }
        "delete" => {
            // delete 的 params 已经包含 id，直接执行
            let result = mysql.do_m(&item.cmdtext, params, &up);
            match result {
                Ok(r) if r.error.is_none() => Ok(()),
                Ok(r) => Err(r.error.unwrap_or_else(|| "操作失败".to_string())),
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
    uname: &str,
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
        Value::String(uname.to_string()),
        Value::String(uptime.to_string()),
    ];

    let up = datastate::MysqlUpInfo::new();
    let result = mysql.do_m_add(sql, params, &up);
    match result {
        Ok(r) if r.error.is_none() => Ok(()),
        Ok(r) => Err(r.error.unwrap_or_else(|| "插入synclog失败".to_string())),
        Err(e) => Err(e),
    }
}

/// 获取待同步记录（返回业务数据）
async fn get(up: &UpInfo, mysql: &Mysql78, expected_cid: &str) -> (StatusCode, Bytes) {
    let limit = up.getnumber as i32;

    // 确保表存在
    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    // 查询 synced=1（已同步到服务器）的记录
    let sql = "SELECT * FROM synclog WHERE synced = 1 AND cid = ? ORDER BY idpk ASC LIMIT ?";
    let params: Vec<Value> = vec![
        Value::String(expected_cid.to_string()),
        Value::Number(limit.into()),
    ];

    let up_info = datastate::MysqlUpInfo::new();
    let rows = match mysql.do_get(sql, params, &up_info) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    // 构建 SynclogItem，对于业务数据需要查询实际表内容
    let mut items: Vec<SynclogItem> = Vec::new();

    for row in rows {
        let tbname = row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let action = row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let idrow = row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // 默认 cmdtext 和 params
        let mut cmdtext = String::new();
        let params = "[]".to_string(); // 下载时不需要参数

        // 如果需要获取业务数据（insert/update），查询业务表
        if action == "insert" || action == "update" {
            if !tbname.is_empty() && !idrow.is_empty() {
                // 查询业务表最新数据
                let business_sql = &format!("SELECT * FROM `{}` WHERE id = ?", tbname);
                let mut business_params = Vec::new();
                business_params.push(Value::String(idrow.clone()));

                if let Ok(business_rows) = mysql.do_get(business_sql, business_params, &up_info) {
                    if let Some(business_row) = business_rows.first() {
                        // 将业务行转换为 JSON 存入 cmdtext
                        if let Ok(json_str) = serde_json::to_string(business_row) {
                            cmdtext = json_str;
                        } else {
                            cmdtext = "{}".to_string();
                        }
                    } else {
                        cmdtext = "{}".to_string();
                    }
                } else {
                    cmdtext = "{}".to_string();
                }
            } else {
                cmdtext = "{}".to_string();
            }
        } else if action == "delete" {
            // delete 操作不返回业务数据（已删除）
            cmdtext = "{}".to_string();
        } else {
            // 未知操作，保持原有 cmdtext（兼容旧数据）
            cmdtext = row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string();
        }

        items.push(SynclogItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: row.get("apisys").and_then(|v| v.as_str()).unwrap_or("v1").to_string(),
            apimicro: row.get("apimicro").and_then(|v| v.as_str()).unwrap_or("iflow").to_string(),
            apiobj: row.get("apiobj").and_then(|v| v.as_str()).unwrap_or("synclog").to_string(),
            tbname,
            action,
            cmdtext,
            params,
            idrow,
            worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            synced: row.get("synced").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            cmdtextmd5: row.get("cmdtextmd5").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        });
    }

    let batch = SynclogBatch { items };
    let bytedata = batch.encode_to_vec();
    use base64::{Engine as _, engine::general_purpose};
    let bytedata_base64 = general_purpose::STANDARD.encode(&bytedata);

    let resp = Response::success_json(&serde_json::json!({ "bytedata": bytedata_base64 }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 获取其他客户端的变更记录（过滤本地worker）
/// 使用idpk作为serverid，实现增量同步
/// 配合5秒安全水位线策略，解决分布式系统时序问题
async fn get_by_worker(up: &UpInfo, mysql: &Mysql78, _expected_cid: &str) -> (StatusCode, Bytes) {
    let limit = up.getnumber as i32;

    let data = match up.parse_prefixed_data() {
        Ok(d) => d,
        Err(_) => {
            let resp = Response::fail("数据解析失败", -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let worker = data.get("worker")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let last_server_id = data.get("lastServerId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if worker.is_empty() {
        let resp = Response::fail("worker参数为空", -1);
        return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    if let Err(e) = ensure_synclog_table(mysql) {
        let resp = Response::fail(&format!("创建synclog表失败: {}", e), -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let safe_interval_secs = 5;
    let max_uptime = chrono::Local::now() - chrono::Duration::seconds(safe_interval_secs);
    let max_uptime_str = max_uptime.format("%Y-%m-%d %H:%M:%S").to_string();

    let sql = "SELECT * FROM synclog WHERE synced = 1 AND worker != ? AND idpk > ? AND uptime <= ? ORDER BY idpk ASC LIMIT ?";
    let params: Vec<Value> = vec![
        Value::String(worker),
        Value::Number(last_server_id.into()),
        Value::String(max_uptime_str),
        Value::Number(limit.into()),
    ];

    let up_info = datastate::MysqlUpInfo::new();
    let rows = match mysql.do_get(sql, params, &up_info) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    // 构建 SynclogItem，对于业务数据需要查询实际表内容
    let mut items: Vec<SynclogItem> = Vec::new();

    for row in rows {
        let tbname = row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let action = row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let idrow = row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // 默认 cmdtext 和 params
        let mut cmdtext = String::new();
        let params = "[]".to_string(); // 下载时不需要参数

        // 如果需要获取业务数据（insert/update），查询业务表
        if action == "insert" || action == "update" {
            if !tbname.is_empty() && !idrow.is_empty() {
                // 查询业务表最新数据
                let business_sql = &format!("SELECT * FROM `{}` WHERE id = ?", tbname);
                let mut business_params = Vec::new();
                business_params.push(Value::String(idrow.clone()));

                if let Ok(business_rows) = mysql.do_get(business_sql, business_params, &up_info) {
                    if let Some(business_row) = business_rows.first() {
                        // 将业务行转换为 JSON 存入 cmdtext
                        if let Ok(json_str) = serde_json::to_string(business_row) {
                            cmdtext = json_str;
                        } else {
                            cmdtext = "{}".to_string();
                        }
                    } else {
                        cmdtext = "{}".to_string();
                    }
                } else {
                    cmdtext = "{}".to_string();
                }
            } else {
                cmdtext = "{}".to_string();
            }
        } else if action == "delete" {
            // delete 操作不返回业务数据（已删除）
            cmdtext = "{}".to_string();
        } else {
            // 未知操作，保持原有 cmdtext（兼容旧数据）
            cmdtext = row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string();
        }

        items.push(SynclogItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: row.get("apisys").and_then(|v| v.as_str()).unwrap_or("v1").to_string(),
            apimicro: row.get("apimicro").and_then(|v| v.as_str()).unwrap_or("iflow").to_string(),
            apiobj: row.get("apiobj").and_then(|v| v.as_str()).unwrap_or("synclog").to_string(),
            tbname,
            action,
            cmdtext,
            params,
            idrow,
            worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            synced: row.get("synced").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            cmdtextmd5: row.get("cmdtextmd5").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        });
    }

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
        lasterrinfo TEXT,
        cmdtextmd5 VARCHAR(64) NOT NULL DEFAULT '',
        cid VARCHAR(64) NOT NULL DEFAULT '',
        upby VARCHAR(100) NOT NULL DEFAULT '',
        uptime DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        INDEX idx_synced (synced),
        INDEX idx_cid_worker (cid, worker)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#;

    let up = datastate::MysqlUpInfo::new();
    mysql.do_m(sql, vec![], &up)
        .map(|_| ())
        .map_err(|e| format!("创建synclog表失败: {}", e))
}
