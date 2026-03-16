//! 多端同步测试
//!
//! 测试方案：
//! 1. 单向下载测试：清空表testtb，服务器数据下载到本地
//! 2. 客户端变更同步：添加2条、修改2条、删除2条，上传synclog，预期服务器一致
//! 3. 服务器变更同步：服务器添加/修改/删除各2条，预期客户端同步后一致
//! 4. 冲突测试

use base::{UpInfo};
use base64::Engine;
use database::Sqlite78;
use prost::Message;

#[derive(Clone, PartialEq, Message)]
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

#[derive(Clone, PartialEq, Message)]
pub struct testtb {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<testtbItem>,
}

#[derive(Clone, PartialEq, Message)]
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
        }

#[derive(Clone, PartialEq, Message)]
pub struct SynclogBatch {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<SynclogItem>,
}

const SERVER_URL: &str = "http://127.0.0.1:3780";
const CID: &str = "test-cid-789";
const WORKER_A: &str = "worker-A";
const WORKER_B: &str = "worker-B";

fn get_server_db_path() -> String {
    let project = base::ProjectPath::find().expect("查找项目根目录失败");
    project.root().join("crates/axum78/tmp/data/remote.db").to_string_lossy().to_string()
}

fn get_client_db_path() -> String {
    let project = base::ProjectPath::find().expect("查找项目根目录失败");
    project.root().join("crates/axum78/tmp/data/client.db").to_string_lossy().to_string()
}

fn ensure_tables(db: &Sqlite78) {
    let up = UpInfo::new();
    let _ = db.do_m(r#"CREATE TABLE IF NOT EXISTS testtb (
        idpk INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL UNIQUE,
        cid TEXT NOT NULL DEFAULT '',
        kind TEXT NOT NULL DEFAULT '',
        item TEXT NOT NULL DEFAULT '',
        data TEXT NOT NULL DEFAULT '',
        upby TEXT NOT NULL DEFAULT '',
        uptime TEXT NOT NULL DEFAULT ''
    )"#, &[], &up);
    
    let _ = db.do_m(r#"CREATE TABLE IF NOT EXISTS synclog (
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
    )"#, &[], &up);
}

fn clear_tables(db: &Sqlite78) {
    let up = UpInfo::new();
    let _ = db.do_m("DELETE FROM testtb", &[], &up);
    let _ = db.do_m("DELETE FROM synclog", &[], &up);
}

fn count_testtb(db: &Sqlite78) -> i32 {
    let up = UpInfo::new();
    let rows = db.do_get("SELECT COUNT(*) as cnt FROM testtb", &[], &up).unwrap_or_default();
    rows.first().and_then(|r| r.get("cnt").and_then(|v| v.as_i64())).unwrap_or(0) as i32
}

async fn download_from_server(sid: &str) -> Result<Vec<testtbItem>, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({"sid": sid, "getnumber": 100}).to_string();
    
    let resp = client
        .post(format!("{}/apitest/testmenu/testtb/get", SERVER_URL))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let res = json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1);
    if res != 0 {
        return Err(json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("未知错误").to_string());
    }
    
    let bytedata = json.get("bytedata").and_then(|v| v.as_array()).ok_or("无bytedata")?;
    let bytes: Vec<u8> = bytedata.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect();
    let result = testtb::decode(&*bytes).map_err(|e| e.to_string())?;
    Ok(result.items)
}

async fn upload_synclog(sid: &str, items: Vec<SynclogItem>) -> Result<i32, String> {
    use base64::{Engine as _, engine::general_purpose};
    
    let client = reqwest::Client::new();
    let batch = SynclogBatch { items };
    let bytedata = batch.encode_to_vec();
    let bytedata_base64 = general_purpose::STANDARD.encode(&bytedata);
    
    let up = serde_json::json!({
        "sid": sid,
        "jsdata": bytedata_base64
    });
    let body = serde_json::to_string(&up).map_err(|e| e.to_string())?;
    
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog/maddmany", SERVER_URL))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let res = json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1);
    if res != 0 {
        return Err(json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("未知错误").to_string());
    }
    
    // jsdata是 JSON字符串，需要解析
    let jsdata_str = json.get("jsdata").and_then(|v| v.as_str()).unwrap_or_default();
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).unwrap_or_default();
    Ok(jsdata.get("batches").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
}

async fn do_work() -> Result<(i32, i32), String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({"sid": "test"}).to_string();
    
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog/dowork", SERVER_URL))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let res = json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1);
    if res != 0 {
        return Err(json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("未知错误").to_string());
    }
    
    let jsdata = json.get("jsdata").ok_or("无jsdata")?;
    Ok((
        jsdata.get("processed").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        jsdata.get("batches").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    ))
}

#[tokio::test]
async fn test_all_plans() {
    println!("\n========== 多端同步测试 ==========");
    
    let sid = format!("{}|{}", CID, WORKER_A);
    let up = UpInfo::new();
    
    // 先初始化数据库（在启动服务器之前）
    let mut server_db = Sqlite78::with_config(&get_server_db_path(), false, false);
    server_db.initialize().expect("服务器数据库初始化失败");
    
    let mut client_db = Sqlite78::with_config(&get_client_db_path(), false, false);
    client_db.initialize().expect("客户端数据库初始化失败");
    
    ensure_tables(&server_db);
    ensure_tables(&client_db);
    clear_tables(&server_db);
    clear_tables(&client_db);
    
    // 启动服务器
    let db_path = get_server_db_path();
    let app = axum78::create_router(&db_path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3780").await.expect("绑定端口失败");
    let _server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("服务器启动失败");
    });
    
    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // ===== 方案1: 单向下载测试 =====
    println!("\n========== 方案1: 单向下载测试 ==========");
    
    // 服务器插入5条数据
    for i in 0..5 {
        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)";
        let _ = server_db.do_m(sql, &[&id as &dyn rusqlite::ToSql, &CID, &format!("server_kind_{}", i), &format!("server_item_{}", i), &format!("server_data_{}", i)], &up);
        println!("服务器插入: {} -> id={}", i, &id[..8]);
    }
    
    let server_count = count_testtb(&server_db);
    println!("服务器记录数: {}", server_count);
    assert_eq!(server_count, 5);
    
    // 客户端下载
    let items = download_from_server(&sid).await.expect("下载失败");
    println!("下载到 {} 条记录", items.len());
    
    for item in &items {
        let sql = "INSERT OR REPLACE INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)";
        let _ = client_db.do_m(sql, &[&item.id as &dyn rusqlite::ToSql, &item.cid, &item.kind, &item.item, &item.data], &up);
    }
    
    let client_count = count_testtb(&client_db);
    println!("客户端记录数: {}", client_count);
    assert_eq!(client_count, 5);
    println!("✅ 方案1通过");
    
    // ===== 方案2: 客户端变更同步 =====
    println!("\n========== 方案2: 客户端变更同步 ==========");
    
    // 获取现有数据用于修改/删除
    let existing_rows = client_db.do_get("SELECT id FROM testtb LIMIT 2", &[], &up).unwrap_or_default();
    let id_for_update = existing_rows.get(0).and_then(|r| r.get("id").and_then(|v| v.as_str())).unwrap_or("").to_string();
    let id_for_delete = existing_rows.get(1).and_then(|r| r.get("id").and_then(|v| v.as_str())).unwrap_or("").to_string();
    
    let mut synclog_items: Vec<SynclogItem> = Vec::new();
    
    // 添加2条
    for i in 0..2 {
        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)";
        let _ = client_db.do_m(sql, &[&id as &dyn rusqlite::ToSql, &CID, &format!("add_kind_{}", i), &format!("add_item_{}", i), &format!("add_data_{}", i)], &up);
        
        synclog_items.push(SynclogItem {
            id: uuid::Uuid::new_v4().to_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: "testtb".to_string(),
            action: "insert".to_string(),
            cmdtext: "INSERT INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)".to_string(),
            params: serde_json::to_string(&[&id, CID, &format!("add_kind_{}", i), &format!("add_item_{}", i), &format!("add_data_{}", i)]).unwrap_or_default(),
            idrow: id.clone(),
            worker: WORKER_A.to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: CID.to_string(),
        });
    }
    println!("客户端添加2条");
    
    // 修改2条
    let sql = "UPDATE testtb SET kind = ?, item = ?, data = ? WHERE id = ?";
    let _ = client_db.do_m(sql, &[&"updated_kind" as &dyn rusqlite::ToSql, &"updated_item", &"updated_data", &id_for_update], &up);
    
    synclog_items.push(SynclogItem {
        id: uuid::Uuid::new_v4().to_string(),
        apisys: "v1".to_string(),
        apimicro: "iflow".to_string(),
        apiobj: "synclog".to_string(),
        tbname: "testtb".to_string(),
        action: "update".to_string(),
        cmdtext: "UPDATE testtb SET kind = ?, item = ?, data = ? WHERE id = ?".to_string(),
        params: serde_json::to_string(&["updated_kind", "updated_item", "updated_data", &id_for_update]).unwrap_or_default(),
        idrow: id_for_update.clone(),
        worker: WORKER_A.to_string(),
        synced: 0,
        cmdtextmd5: String::new(),
        cid: CID.to_string(),
    });
    println!("客户端修改1条");
    
    // 删除1条
    let sql = "DELETE FROM testtb WHERE id = ?";
    let _ = client_db.do_m(sql, &[&id_for_delete as &dyn rusqlite::ToSql], &up);
    
    synclog_items.push(SynclogItem {
        id: uuid::Uuid::new_v4().to_string(),
        apisys: "v1".to_string(),
        apimicro: "iflow".to_string(),
        apiobj: "synclog".to_string(),
        tbname: "testtb".to_string(),
        action: "delete".to_string(),
        cmdtext: "DELETE FROM testtb WHERE id = ?".to_string(),
        params: serde_json::to_string(&[&id_for_delete]).unwrap_or_default(),
        idrow: id_for_delete.clone(),
        worker: WORKER_A.to_string(),
        synced: 0,
        cmdtextmd5: String::new(),
        cid: CID.to_string(),
    });
    println!("客户端删除1条");
    
    // 上传synclog
    let batches = upload_synclog(&sid, synclog_items).await.expect("上传失败");
    println!("上传 {} 条synclog", batches);
    
    // 执行doWork
    let (processed, _) = do_work().await.expect("doWork失败");
    println!("doWork处理 {} 条", processed);
    
    // 验证服务器数据
    let server_count = count_testtb(&server_db);
    let client_count = count_testtb(&client_db);
    println!("客户端记录数: {}", client_count);
    println!("服务器记录数: {}", server_count);
    assert_eq!(client_count, server_count);
    println!("✅ 方案2通过");
    
    // ===== 方案3: 服务器变更同步 =====
    println!("\n========== 方案3: 服务器变更同步 ==========");
    
    // 模拟另一个客户端(worker-B)上传变更
    let worker_b_sid = format!("{}|{}", CID, WORKER_B);
    
    // 服务器添加2条 (通过worker-B上传synclog，然后doWork执行)
    let mut synclog_items_b: Vec<SynclogItem> = Vec::new();
    for i in 0..2 {
        let id = uuid::Uuid::new_v4().to_string();
        
        synclog_items_b.push(SynclogItem {
            id: uuid::Uuid::new_v4().to_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: "testtb".to_string(),
            action: "insert".to_string(),
            cmdtext: "INSERT INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)".to_string(),
            params: serde_json::to_string(&[&id, CID, &format!("server_add_kind_{}", i), &format!("server_add_item_{}", i), &format!("server_add_data_{}", i)]).unwrap_or_default(),
            idrow: id.clone(),
            worker: WORKER_B.to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: CID.to_string(),
        });
    }
    println!("服务器添加2条");
    
    // 上传synclog (通过worker-B)
    let batches = upload_synclog(&worker_b_sid, synclog_items_b).await.expect("上传失败");
    println!("上传 {} 条synclog", batches);
    
    // 执行doWork (这会在服务器端执行INSERT)
    let (processed, _) = do_work().await.expect("doWork失败");
    println!("doWork处理 {} 条", processed);
    
    // 客户端下载synclog (worker != WORKER_A)
    let client = reqwest::Client::new();
    let body = serde_json::json!({"sid": format!("{}|{}", CID, WORKER_A), "getnumber": 100}).to_string();
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog/get", SERVER_URL))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("请求失败");
    
    let json: serde_json::Value = resp.json().await.expect("解析失败");
    let res = json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1);
    if res != 0 {
        panic!("下载synclog失败: {:?}", json.get("errmsg"));
    }
    
    // 打印原始响应
    println!("服务器响应: {:?}", json);
    
    // jsdata是JSON字符串，里面包含bytedata(base64编码)
    let jsdata_str = json.get("jsdata").and_then(|v| v.as_str()).expect("无jsdata");
    println!("jsdata字符串: {}", jsdata_str);
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).expect("解析jsdata失败");
    let bytedata_base64 = jsdata.get("bytedata").and_then(|v| v.as_str()).expect("无bytedata");
    println!("bytedata_base64长度: {}", bytedata_base64.len());
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytedata_base64.as_bytes()).expect("Base64解码失败");
    println!("解码后字节长度: {}", bytes.len());
    let synclog_batch = SynclogBatch::decode(&*bytes).expect("解码失败");
    
    println!("下载到 {} 条synclog", synclog_batch.items.len());
    
    // 执行synclog中的SQL
    for item in &synclog_batch.items {
        let params: Vec<serde_json::Value> = serde_json::from_str(&item.params).unwrap_or_default();
        println!("执行: action={}, params={:?}", item.action, params);
        match item.action.as_str() {
            "insert" => {
                let sql = "INSERT OR REPLACE INTO testtb (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)";
                let p0 = params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p1 = params.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p2 = params.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p3 = params.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p4 = params.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let result = client_db.do_m(sql, &[&p0 as &dyn rusqlite::ToSql, &p1, &p2, &p3, &p4], &up);
                println!("INSERT结果: {:?}, id={}", result, p0);
            }
            "update" => {
                let sql = "UPDATE testtb SET kind = ?, item = ?, data = ? WHERE id = ?";
                let p0 = params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p1 = params.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p2 = params.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p3 = params.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = client_db.do_m(sql, &[&p0 as &dyn rusqlite::ToSql, &p1, &p2, &p3], &up);
            }
            "delete" => {
                let sql = "DELETE FROM testtb WHERE id = ?";
                let p0 = params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = client_db.do_m(sql, &[&p0 as &dyn rusqlite::ToSql], &up);
            }
            _ => {}
        }
    }
    
    let server_count = count_testtb(&server_db);
    let client_count = count_testtb(&client_db);
    println!("客户端记录数: {}", client_count);
    println!("服务器记录数: {}", server_count);
    assert_eq!(client_count, server_count);
    println!("✅ 方案3通过");
    
    println!("\n========== 所有测试通过 ==========");
}
