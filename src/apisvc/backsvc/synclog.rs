//! synclog API实现
//!
//! 路径: apisvc/backsvc/synclog
//! 路由: POST /apisvc/backsvc/synclog/:apifun
//!
//! 参考 LOGSVC 实现
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

/// synclog 单项记录
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct SynclogItem {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub tbname: String,
    #[prost(string, tag = "3")]
    pub action: String,
    #[prost(string, tag = "4")]
    pub cmdtext: String,
    #[prost(string, tag = "5")]
    pub params: String,
    #[prost(string, tag = "6")]
    pub idrow: String,
    #[prost(string, tag = "7")]
    pub cid: String,
}

/// synclog 批量数据
#[derive(Clone, PartialEq, Message)]
pub struct SynclogBatch {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<SynclogItem>,
}

/// doWork 响应
#[derive(Clone, PartialEq, Message, Serialize, Deserialize)]
pub struct DoWorkResult {
    #[prost(int32, tag = "1")]
    pub processed: i32,
    #[prost(int32, tag = "2")]
    pub batches: i32,
}

// ============ API实现 ============

/// 处理synclog API请求
pub async fn handle(apifun: &str, up: UpInfo, db: &Sqlite78) -> (StatusCode, Bytes) {
    match apifun.to_lowercase().as_str() {
        "maddmany" => m_add_many(&up, db).await,
        "dowork" => do_work(db).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// mAddMany - 批量上传同步记录
async fn m_add_many(up: &UpInfo, db: &Sqlite78) -> (StatusCode, Bytes) {
    let expected_cid = if up.sid.is_empty() {
        let resp = Response::fail("无效的SID", -1);
        return (StatusCode::UNAUTHORIZED, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    } else if up.sid.contains('|') {
        up.sid.split('|').next().unwrap_or("").to_string()
    } else {
        up.sid.clone()
    };

    let batch: SynclogBatch = match &up.bytedata {
        Some(data) => match SynclogBatch::decode(&**data) {
            Ok(b) => b,
            Err(_) => {
                let resp = Response::fail("解码同步数据失败", -1);
                return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
            }
        },
        None => {
            let resp = Response::fail("无同步数据", -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };

    ensure_synclog_table(db);

    let mut batches = 0i32;
    for item in &batch.items {
        if !item.cid.is_empty() && item.cid != expected_cid {
            continue;
        }

        let id = if item.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { item.id.clone() };
        
        let sql = "INSERT INTO synclog (id, tbname, action, cmdtext, params, idrow, cid, synced) VALUES (?, ?, ?, ?, ?, ?, ?, 0)";
        
        let _ = db.do_m_add(
            sql,
            &[
                &id as &dyn rusqlite::ToSql,
                &item.tbname,
                &item.action,
                &item.cmdtext,
                &item.params,
                &item.idrow,
                &expected_cid,
            ],
            up,
        );
        batches += 1;
    }

    let resp = Response::success_json(&serde_json::json!({ "batches": batches }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// doWork - 执行同步操作
async fn do_work(db: &Sqlite78) -> (StatusCode, Bytes) {
    ensure_synclog_table(db);
    ensure_testtb_table(db);

    let up = UpInfo::new();
    let limit = 100;
    let max_batches = 10;
    let mut total_processed = 0i32;
    let mut batch_count = 0i32;

    for _ in 0..max_batches {
        let rows = match db.do_get(
            "SELECT * FROM synclog WHERE synced = 0 ORDER BY idpk ASC LIMIT ?",
            &[&limit as &dyn rusqlite::ToSql],
            &up,
        ) {
            Ok(r) => r,
            Err(_) => break,
        };

        if rows.is_empty() {
            break;
        }

        batch_count += 1;

        for row in &rows {
            let idpk = row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let tbname = row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let action = row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let params_str = row.get("params").and_then(|v| v.as_str()).unwrap_or("[]").to_string();
            let idrow = row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string();

            let result = process_synclog_item(db, &up, &tbname, &action, &params_str, &idrow);
            
            let (synced, lasterr) = match result {
                Ok(_) => (1, String::new()),
                Err(e) => (-1, e),
            };
            
            let _ = db.do_m(
                "UPDATE synclog SET synced = ?, lasterrinfo = ? WHERE idpk = ?",
                &[&synced as &dyn rusqlite::ToSql, &lasterr, &idpk],
                &up,
            );

            if synced == 1 {
                total_processed += 1;
            }
        }
    }

    let result = DoWorkResult {
        processed: total_processed,
        batches: batch_count,
    };
    let resp = Response::success_json(&result);
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

fn ensure_synclog_table(db: &Sqlite78) {
    let up = UpInfo::new();
    let sql = r#"CREATE TABLE IF NOT EXISTS synclog (
        idpk INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL UNIQUE,
        tbname TEXT NOT NULL DEFAULT '',
        action TEXT NOT NULL DEFAULT '',
        cmdtext TEXT NOT NULL DEFAULT '',
        params TEXT NOT NULL DEFAULT '[]',
        idrow TEXT NOT NULL DEFAULT '',
        cid TEXT NOT NULL DEFAULT '',
        synced INTEGER NOT NULL DEFAULT 0,
        lasterrinfo TEXT NOT NULL DEFAULT '',
        upby TEXT NOT NULL DEFAULT '',
        uptime TEXT NOT NULL DEFAULT ''
    )"#;
    let _ = db.do_m(sql, &[], &up);
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

fn process_synclog_item(
    db: &Sqlite78,
    up: &UpInfo,
    tbname: &str,
    action: &str,
    params_str: &str,
    idrow: &str,
) -> Result<(), String> {
    let params: Vec<serde_json::Value> = serde_json::from_str(params_str).unwrap_or_default();

    match action {
        "insert" => {
            if tbname != "testtb" {
                return Err(format!("不支持的表: {}", tbname));
            }
            let id = params.get(0).and_then(|v| v.as_str()).unwrap_or("");
            let cid = params.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let kind = params.get(2).and_then(|v| v.as_str()).unwrap_or("");
            let item = params.get(3).and_then(|v| v.as_str()).unwrap_or("");
            let data = params.get(4).and_then(|v| v.as_str()).unwrap_or("");

            let new_id = if id.is_empty() { uuid::Uuid::new_v4().to_string() } else { id.to_string() };
            
            db.do_m(
                "INSERT OR REPLACE INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)",
                &[&new_id as &dyn rusqlite::ToSql, &cid, &kind, &item, &data],
                up,
            ).map(|_| ()).map_err(|e| e)
        }
        "update" => {
            if tbname != "testtb" {
                return Err(format!("不支持的表: {}", tbname));
            }
            let kind = params.get(0).and_then(|v| v.as_str()).unwrap_or("");
            let item = params.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let data = params.get(2).and_then(|v| v.as_str()).unwrap_or("");
            let id = params.get(3).and_then(|v| v.as_str()).unwrap_or(idrow);

            db.do_m(
                "UPDATE testtb SET kind = ?, item = ?, data = ? WHERE id = ?",
                &[&kind as &dyn rusqlite::ToSql, &item, &data, &id],
                up,
            ).map(|_| ()).map_err(|e| e)
        }
        "delete" => {
            if tbname != "testtb" {
                return Err(format!("不支持的表: {}", tbname));
            }
            db.do_m(
                "DELETE FROM testtb WHERE id = ?",
                &[&idrow as &dyn rusqlite::ToSql],
                up,
            ).map(|_| ()).map_err(|e| e)
        }
        _ => Err(format!("未知的action: {}", action)),
    }
}
