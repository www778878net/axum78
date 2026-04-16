//! synclog API实现
//!
//! 路径: apisvc/backsvc/synclog
//! 路由: POST /apisvc/backsvc/synclog/:apifun
//!
//! 多端分布式同步：
//! - 客户端和服务端使用同一个 synclog 表
//! - 通过 worker 字段区分不同客户端
//! - 下载时只获取 worker != 本地的记录
//!
//! 数据库由DataState基类自己控制，使用默认数据库路径

use axum::{
    body::Bytes,
    http::StatusCode,
};
use base::{UpInfo, Response};
use datastate::LocalDB;
use datastate::datastate::DataState;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    let expected_cid = if up.sid.is_empty() {
        let resp = Response::fail("无效的SID", -1);
        return (StatusCode::UNAUTHORIZED, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    } else if up.sid.contains('|') {
        up.sid.split('|').next().unwrap_or("").to_string()
    } else {
        up.sid.clone()
    };

    let batch: SynclogBatch = if let Some(data) = &up.bytedata {
        match SynclogBatch::decode(&**data) {
            Ok(b) => b,
            Err(e) => {
                let resp = Response::fail(&format!("解码失败: {}", e), -1);
                return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
            }
        }
    } else if let Some(jsdata) = &up.jsdata {
        use base64::{Engine as _, engine::general_purpose};
        match general_purpose::STANDARD.decode(jsdata) {
            Ok(bytes) => match SynclogBatch::decode(&*bytes) {
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

    ensure_synclog_table(db);

    let mut batches = 0;
    for item in batch.items {
        let id = if item.id.is_empty() { datastate::next_id_string() } else { item.id.clone() };

        let sql = "INSERT INTO synclog (id, apisys, apimicro, apiobj, tbname, action, cmdtext, params, idrow, worker, synced, cmdtextmd5, cid, upby) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)";

        let _ = db.execute_with_params(
            sql,
            &[
                &id as &dyn rusqlite::ToSql,
                &item.apisys,
                &item.apimicro,
                &item.apiobj,
                &item.tbname,
                &item.action,
                &item.cmdtext,
                &item.params,
                &item.idrow,
                &item.worker,
                &item.cmdtextmd5,
                &expected_cid,
                &item.upby,
            ],
        );
        batches += 1;
    }

    let resp = Response::success_json(&serde_json::json!({ "batches": batches }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

async fn do_work(up: &UpInfo, db: &LocalDB) -> (StatusCode, Bytes) {
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

    ensure_synclog_table(db);

    let count_rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match db.query("SELECT COUNT(*) as cnt FROM synclog WHERE worker = ?", &[&worker as &dyn rusqlite::ToSql]) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询synclog表失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    let total_count = count_rows.first().and_then(|r| r.get("cnt")).and_then(|v| v.as_i64()).unwrap_or(0);

    let pending_rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match db.query("SELECT COUNT(*) as cnt FROM synclog WHERE synced = 0 AND worker = ?", &[&worker as &dyn rusqlite::ToSql]) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询pending记录失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    let pending_count = pending_rows.first().and_then(|r| r.get("cnt")).and_then(|v| v.as_i64()).unwrap_or(0);

    println!("[doWork] worker={}, synclog表总记录: {}, 待处理: {}", worker, total_count, pending_count);

    let limit = 100;
    let max_batches = 10;
    let mut total_processed = 0i32;
    let mut batch_count = 0i32;

    for _ in 0..max_batches {
        let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match db.query(
            "SELECT * FROM synclog WHERE synced = 0 AND worker = ? ORDER BY idpk ASC LIMIT ?",
            &[&worker as &dyn rusqlite::ToSql, &limit as &dyn rusqlite::ToSql],
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[doWork] 查询失败: {}", e);
                break;
            }
        };

        println!("[doWork] 查询返回 {} 条记录", rows.len());

        if rows.is_empty() {
            println!("[doWork] 没有待处理记录");
            break;
        }

        batch_count += 1;
        println!("[doWork] 批次 {} 处理 {} 条记录", batch_count, rows.len());

        for row in &rows {
            let idpk = row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let tbname = row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let action = row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let params_str = {
                match row.get("params") {
                    Some(Value::String(s)) => s.clone(),
                    Some(v @ Value::Array(_)) => serde_json::to_string(&v).unwrap_or_default(),
                    _ => "[]".to_string(),
                }
            };
            let cmdtext = row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let idrow = row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let upby = row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let synclog_uptime = row.get("uptime").and_then(|v| v.as_str()).unwrap_or("").to_string();

            println!("[doWork] 处理记录: idpk={}, tbname={}, action={}, idrow={}", idpk, tbname, action, idrow);

            let result = process_synclog_item(db, &upby, &tbname, &action, &params_str, &idrow, &synclog_uptime, &cmdtext);
            let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            match result {
                Ok(_) => {
                    println!("[doWork] 处理成功: idpk={}", idpk);
                    let _ = db.execute_with_params(
                        "UPDATE synclog SET synced = 1, lasterrinfo = '', uptime = ? WHERE idpk = ?",
                        &[&now as &dyn rusqlite::ToSql, &idpk],
                    );
                    total_processed += 1;
                }
                Err(e) => {
                    println!("[doWork] 处理失败: idpk={}, error={}", idpk, e);
                    let _ = db.execute_with_params(
                        "UPDATE synclog SET synced = -1, lasterrinfo = ?, uptime = ? WHERE idpk = ?",
                        &[&e as &dyn rusqlite::ToSql, &now, &idpk],
                    );
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

async fn get(up: &UpInfo, db: &LocalDB) -> (StatusCode, Bytes) {
    ensure_synclog_table(db);

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

    let now = chrono::Utc::now().timestamp();
    let safe_time = now - 5;
    let safe_datetime = chrono::DateTime::from_timestamp(safe_time, 0)
        .unwrap_or_else(|| chrono::Utc::now())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = match db.query(
        "SELECT * FROM synclog WHERE synced = 1 AND worker != ? AND idpk > ? AND uptime <= ? ORDER BY idpk ASC LIMIT ?",
        &[&worker as &dyn rusqlite::ToSql, &last_server_id, &safe_datetime, &limit as &dyn rusqlite::ToSql],
    ) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::fail(&format!("查询失败: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    let items: Vec<SynclogItem> = rows
        .iter()
        .map(|row| SynclogItem {
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

    let batch = SynclogBatch { items };
    let bytedata = batch.encode_to_vec();
    use base64::{Engine as _, engine::general_purpose};
    let bytedata_base64 = general_purpose::STANDARD.encode(&bytedata);
    let resp = Response::success_json(&serde_json::json!({
        "bytedata": bytedata_base64
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

fn ensure_synclog_table(db: &LocalDB) {
    let sql = r#"CREATE TABLE IF NOT EXISTS synclog (
        idpk INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL UNIQUE,
        apisys TEXT NOT NULL DEFAULT 'v1',
        apimicro TEXT NOT NULL DEFAULT 'iflow',
        apiobj TEXT NOT NULL DEFAULT 'synclog',
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
    let _ = db.execute(sql);
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

fn process_synclog_item(
    db: &LocalDB,
    _upby: &str,
    tbname: &str,
    action: &str,
    params_str: &str,
    idrow: &str,
    _synclog_uptime: &str,
    cmdtext: &str,
) -> Result<(), String> {
    // 使用 DataState 通用方法处理所有表
    let datastate = DataState::with_db(tbname, db.clone());

    match action {
        "insert" => {
            let columns = parse_columns_from_cmdtext(cmdtext, action)?;
            let params: Vec<Value> = serde_json::from_str(params_str).unwrap_or_default();
            let record = build_record_from_params(&columns, params);
            datastate.datasync.m_sync_save(&record)
                .map(|_| ())
                .map_err(|e| e)
        }
        "update" => {
            let columns = parse_columns_from_cmdtext(cmdtext, action)?;
            let params: Vec<Value> = serde_json::from_str(params_str).unwrap_or_default();
            let record = build_record_from_params(&columns, params);
            // UPDATE 的 params 最后一个是 id (WHERE 条件)
            let id = record.get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(idrow)
                .to_string();
            datastate.datasync.m_sync_update(&id, &record)
                .map(|_| ())
                .map_err(|e| e)
        }
        "delete" => {
            datastate.datasync.m_sync_del(idrow)
                .map(|_| ())
                .map_err(|e| e)
        }
        _ => Err(format!("未知的action: {}", action)),
    }
}
