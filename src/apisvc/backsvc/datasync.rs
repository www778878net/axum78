//! datasync API实现
//!
//! 路径: apisvc/backsvc/datasync
//! 路由: POST /apisvc/backsvc/datasync/:apifun
//!
//! 多端分布式同步：
//! - 客户端和服务端使用同一个 datasync 表
//! - 通过 worker 字段区分不同客户端
//! - 下载时只获取 worker != 本地的记录
//!
//! 数据库由DataState基类自己控制，使用默认数据库路径

use axum::{
    body::Bytes,
    http::{Method, StatusCode},
};
use base::{UpInfo, Response};
use datastate::LocalDB;
use datastate::datastate::DataState;
use crate::router::Controller78;
use async_trait::async_trait;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct DatasyncItem {
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
pub struct DatasyncBatch {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<DatasyncItem>,
}

pub async fn handle(apifun: &str, up: UpInfo) -> (StatusCode, Bytes) {
    let remote_db_path = "docs/config/remote.db";
    let db = match LocalDB::with_path(remote_db_path) {
        Ok(d) => d,
        Err(e) => {
            let resp = Response::fail(&format!("数据库初始化失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    match apifun.to_lowercase().as_str() {
        "maddmany" => m_add_many(&up, &db).await,
        "dowork" => do_work(&up, &db).await,
        "get" => get(&up, &db).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

async fn m_add_many(up: &UpInfo, db: &LocalDB) -> (StatusCode, Bytes) {
    let batch: DatasyncBatch = if let Some(data) = &up.bytedata {
        match DatasyncBatch::decode(&**data) {
            Ok(b) => b,
            Err(e) => {
                let resp = Response::fail(&format!("解码失败: {}", e), -1);
                return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
            }
        }
    } else if let Some(jsdata) = &up.jsdata {
        use base64::{Engine as _, engine::general_purpose};
        match general_purpose::STANDARD.decode(jsdata) {
            Ok(bytes) => match DatasyncBatch::decode(&*bytes) {
                Ok(b) => b,
                Err(e) => {
                    let resp = Response::fail(&format!("解码失败: {}", e), -1);
                    return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
                }
            },
            Err(e) => {
                let resp = Response::fail(&format!("Base64解码失败: {}", e), -1);
                return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
            }
        }
    } else {
        let resp = Response::fail("无bytedata或jsdata", -1);
        return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    };

    ensure_datasync_table(db).await;

    let mut batches = 0;
    println!("[maddmany] items count: {}", batch.items.len());
    for item in batch.items {
        println!("[maddmany] item: id={}, tb={}, act={}, idrow={}", item.id, item.tbname, item.action, item.idrow);
        let id = if item.id.is_empty() { datastate::next_id_string() } else { item.id.clone() };

        let sql = "INSERT INTO datasync (id, apisys, apimicro, apiobj, tbname, action, cmdtext, params, idrow, worker, synced, cmdtextmd5, cid, upby) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)";

        let exec_result = db.execute_with_params(
            sql,
            vec![
                rusqlite::types::Value::Text(id.clone()),
                rusqlite::types::Value::Text(item.apisys.clone()),
                rusqlite::types::Value::Text(item.apimicro.clone()),
                rusqlite::types::Value::Text(item.apiobj.clone()),
                rusqlite::types::Value::Text(item.tbname.clone()),
                rusqlite::types::Value::Text(item.action.clone()),
                rusqlite::types::Value::Text(item.cmdtext.clone()),
                rusqlite::types::Value::Text(item.params.clone()),
                rusqlite::types::Value::Text(item.idrow.clone()),
                rusqlite::types::Value::Text(item.worker.clone()),
                rusqlite::types::Value::Text(item.cmdtextmd5.clone()),
                rusqlite::types::Value::Text(item.cid.clone()),
                rusqlite::types::Value::Text(item.upby.clone()),
            ],
        ).await;
        match &exec_result {
            Ok(_) => println!("[maddmany] INSERT OK: id={}", id),
            Err(e) => eprintln!("[maddmany] INSERT FAIL: id={}, err={}", id, e),
        }
        batches += 1;
    }

    let resp = Response::success_json(&serde_json::json!({ "batches": batches }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

// do_work: 重放本地 datasync 表中待处理的记录(synced=0)到业务表
// 流程: 取出 synced=0 的记录 -> 解析 cmdtext/params -> 重放到 tbname -> 标记 synced=1(成功)/-1(失败)
async fn do_work(up: &UpInfo, db: &LocalDB) -> (StatusCode, Bytes) {
    ensure_datasync_table(db).await;

    let dbc = db.clone();
    let rows: Vec<std::collections::HashMap<String, serde_json::Value>> =
        match tokio::task::spawn_blocking(move || {
            dbc.query_sync("SELECT * FROM datasync WHERE synced = 0 ORDER BY id ASC", &[])
        }).await.map_err(|e| format!("spawn_blocking: {}", e)).and_then(|r| r) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询待重放记录失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let mut processed = 0i32;
    let mut success = 0i32;
    let mut failed = 0i32;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    for row in &rows {
        let item = row_to_datasync_item(row);
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let replay = process_datasync_item(
            db,
            &up.uname,
            &item.tbname,
            &item.action,
            &item.params,
            &item.idrow,
            &now,
            &item.cmdtext,
        ).await;

        let (new_synced, lasterrinfo) = match &replay {
            Ok(_) => (1i64, String::new()),
            Err(e) => {
                errors.push(serde_json::json!({ "id": item.id, "idrow": item.idrow, "error": e }));
                (-1i64, e.clone())
            }
        };

        let update_sql = "UPDATE datasync SET synced = ?, lasterrinfo = ?, uptime = ? WHERE id = ?";
        let _ = db.execute_with_params(
            update_sql,
            vec![
                rusqlite::types::Value::Integer(new_synced),
                rusqlite::types::Value::Text(lasterrinfo),
                rusqlite::types::Value::Text(now),
                rusqlite::types::Value::Text(item.id.clone()),
            ],
        ).await;

        processed += 1;
        if replay.is_ok() { success += 1; } else { failed += 1; }
    }

    let resp = Response::success_json(&serde_json::json!({
        "processed": processed,
        "success": success,
        "failed": failed,
        "errors": errors,
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 从一行 HashMap 解析为 DatasyncItem（params 取原始文本，用于重放）
fn row_to_datasync_item(row: &std::collections::HashMap<String, serde_json::Value>) -> DatasyncItem {
    let get_str = |k: &str| -> String {
        row.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };
    DatasyncItem {
        id: get_str("id"),
        apisys: get_str("apisys"),
        apimicro: get_str("apimicro"),
        apiobj: get_str("apiobj"),
        tbname: get_str("tbname"),
        action: get_str("action"),
        cmdtext: get_str("cmdtext"),
        params: get_str("params"),
        idrow: get_str("idrow"),
        worker: get_str("worker"),
        synced: row.get("synced").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        cmdtextmd5: get_str("cmdtextmd5"),
        cid: get_str("cid"),
        upby: get_str("upby"),
    }
}

// ====== 辅助函数 ======

async fn get(up: &UpInfo, db: &LocalDB) -> (StatusCode, Bytes) {
    let expected_worker = if up.sid.contains('|') {
        up.sid.split('|').nth(1).unwrap_or("").to_string()
    } else {
        String::new()
    };

    ensure_datasync_table(db).await;

    let limit = up.getnumber as i32;
    let cid = up.cid.clone();
    let w = expected_worker.clone();
    let dbc = db.clone();
    let rows = match tokio::task::spawn_blocking(move || {
        dbc.query_sync("SELECT * FROM datasync WHERE synced=1 AND cid=? AND worker!=? ORDER BY id ASC LIMIT ?",
            &[&cid as &dyn rusqlite::ToSql, &w as &dyn rusqlite::ToSql, &limit as &dyn rusqlite::ToSql])
    }).await.map_err(|e| format!("spawn_blocking: {}", e)).and_then(|r| r) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let items: Vec<DatasyncItem> = rows
        .iter()
        .map(|row| DatasyncItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: row.get("apisys").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apimicro: row.get("apimicro").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apiobj: row.get("apiobj").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cmdtext: row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            params: {
                match row.get("params") {
                    Some(Value::String(s)) => s.clone(),
                    Some(v @ Value::Array(_)) | Some(v @ Value::Object(_)) => serde_json::to_string(&v).unwrap_or_default(),
                    _ => String::new(),
                }
            },
            idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            synced: row.get("synced").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            cmdtextmd5: row.get("cmdtextmd5").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        })
        .collect();

    let batch = DatasyncBatch { items };
    let bytedata = batch.encode_to_vec();
    use base64::{Engine as _, engine::general_purpose};
    let bytedata_base64 = general_purpose::STANDARD.encode(&bytedata);
    let resp = Response::success_json(&serde_json::json!({
        "bytedata": bytedata_base64
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

async fn ensure_datasync_table(db: &LocalDB) {
    let sql = r#"CREATE TABLE IF NOT EXISTS datasync (
        id TEXT PRIMARY KEY,
        apisys TEXT NOT NULL DEFAULT 'v1',
        apimicro TEXT NOT NULL DEFAULT 'iflow',
        apiobj TEXT NOT NULL DEFAULT 'datasync',
        tbname TEXT NOT NULL DEFAULT '',
        action TEXT NOT NULL DEFAULT '',
        cmdtext TEXT NOT NULL DEFAULT '',
        params TEXT NOT NULL DEFAULT '[]',
        idrow TEXT NOT NULL DEFAULT '',
        worker TEXT NOT NULL DEFAULT '',
        synced INTEGER NOT NULL DEFAULT 0,
        lasterrinfo TEXT NOT NULL DEFAULT '',
        cmdtextmd5 TEXT NOT NULL DEFAULT '',
        cid TEXT NOT NULL DEFAULT '',
        upby TEXT NOT NULL DEFAULT '',
        uptime TEXT NOT NULL DEFAULT ''
    )"#;
    let _ = db.execute_with_params(sql, vec![]).await;
}

/// 从 cmdtext 解析列名
/// INSERT: INSERT INTO `table` (`col1`, `col2`) VALUES (?, ?)
/// UPDATE: UPDATE `table` SET `col1` = ?, `col2` = ? WHERE `id` = ?
fn parse_columns_from_cmdtext(cmdtext: &str, action: &str) -> Result<Vec<String>, String> {
    let mut columns = Vec::new();

    match action {
        "insert" => {
            // INSERT INTO `table` (`col1`, `col2`) VALUES (?, ?)
            if let Some(start) = cmdtext.find('(') {
                if let Some(end) = cmdtext.find(')') {
                    let cols_part = &cmdtext[start + 1..end];
                    for col in cols_part.split(',') {
                        let col = col.trim();
                        // 提取反引号中的列名
                        if col.starts_with('`') && col.ends_with('`') {
                            columns.push(col[1..col.len()-1].to_string());
                        }
                    }
                }
            }
        }
        "update" => {
            // UPDATE `table` SET `col1` = ?, `col2` = ? WHERE `id` = ?
            if let Some(set_start) = cmdtext.find("SET ") {
                let set_end = cmdtext.find(" WHERE").unwrap_or(cmdtext.len());
                let set_clause = &cmdtext[set_start + 4..set_end];

                for part in set_clause.split(',') {
                    let part = part.trim();
                    // 格式：`col` = ?
                    if let Some(start) = part.find('`') {
                        if let Some(end) = part[start + 1..].find('`') {
                            columns.push(part[start + 1..start + 1 + end].to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }

    if columns.is_empty() && action != "delete" {
        return Err(format!("无法从 cmdtext 解析列名: {}", cmdtext));
    }

    Ok(columns)
}

/// 将 params 数组和列名转换为 HashMap
fn build_record_from_params(columns: &[String], params: Vec<Value>) -> std::collections::HashMap<String, Value> {
    let mut record = std::collections::HashMap::new();
    for (i, col) in columns.iter().enumerate() {
        if i < params.len() {
            record.insert(col.clone(), params[i].clone());
        }
    }
    record
}

async fn process_datasync_item(
    db: &LocalDB,
    _upby: &str,
    tbname: &str,
    action: &str,
    params_str: &str,
    idrow: &str,
    _datasync_uptime: &str,
    cmdtext: &str,
) -> Result<(), String> {
    // 使用 DataState 通用方法处理所有表
    let datastate = DataState::with_db(tbname, db.clone());

    match action {
        "insert" => {
            let columns = parse_columns_from_cmdtext(cmdtext, action)?;
            let params: Vec<Value> = serde_json::from_str(params_str).unwrap_or_default();
            let record = build_record_from_params(&columns, params);
            tokio::spawn(async move {
                datastate.datasync.m_sync_save(&record).await
                    .map(|_| ())
            }).await.map_err(|e| format!("spawn failed: {}", e))?
                .map_err(|e| e)
        }
        "update" => {
            let columns = parse_columns_from_cmdtext(cmdtext, action)?;
            let params: Vec<Value> = serde_json::from_str(params_str).unwrap_or_default();
            let record = build_record_from_params(&columns, params);
            let id = record.get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(idrow)
                .to_string();
            tokio::spawn(async move {
                datastate.datasync.m_sync_update(&id, &record).await
                    .map(|_| ())
            }).await.map_err(|e| format!("spawn failed: {}", e))?
                .map_err(|e| e)
        }
        "delete" => {
            let idrow = idrow.to_string();
            tokio::spawn(async move {
                datastate.datasync.m_sync_del(&idrow).await
                    .map(|_| ())
            }).await.map_err(|e| format!("spawn failed: {}", e))?
                .map_err(|e| e)
        }
        _ => Err(format!("未知的action: {}", action)),
    }
}

// ====== Controller78 实现 ======

pub struct DatasyncController;

#[async_trait]
impl Controller78 for DatasyncController {
    async fn call(&self, up: &mut UpInfo, fun: &str, _method: &Method) -> Value {
        // 复用现有 handle（up 需 by-value，clone 一下）
        let up_clone = up.clone();
        let (_status, bytes) = handle(fun, up_clone).await;
        let resp: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        // 把 handle 的响应映射到 Controller78 的协议
        if let Some(res) = resp.get("res").and_then(|v| v.as_i64()) {
            if res != 0 {
                up.res = res as i32;
                up.errmsg = resp.get("errmsg").and_then(|v| v.as_str()).unwrap_or("").to_string();
                return Value::Null;
            }
        }

        // back 字段可能是 JSON 字符串或对象
        resp.get("back").and_then(|v| {
            if let Some(s) = v.as_str() {
                serde_json::from_str(s).ok()
            } else {
                Some(v.clone())
            }
        }).unwrap_or(Value::Null)
    }
}

/// 注册到全局路由表（调用一次即可）
pub fn register_controller() {
    let path = format!("apisvc/backsvc/datasync");
    crate::router::registry::register(&path, Arc::new(DatasyncController));
    // 也注册别名（兼容大小写）
    crate::router::registry::register(&path.to_lowercase(), Arc::new(DatasyncController));
}
