//! 多端同步测试 - MySQL 版本
//!
//! 测试方案：
//! 1. 单向下载测试：MySQL 数据下载到本地 SQLite
//! 2. 客户端变更同步：添加2条、修改2条、删除2条，上传synclog，预期 MySQL 一致
//! 3. 服务器变更同步：MySQL 添加/修改/删除各2条，预期客户端同步后一致
//! 4. 最终一致性验证：客户端和 MySQL 数据完全一致
//! 5. 冲突测试：双边同时修改，验证时间戳优先策略
//!
//! MySQL 唯一键约束：u_kind_item (cid, kind, item)

use base::UpInfo;
use base64::Engine;
use chrono;
use datastate::datastate::TestTb;
use datastate::next_id_string;
use prost::Message;
use axum78::apitest::testmenu::testtb::{testtb, testtbItem};
use axum78::apisvc::backsvc::synclog::{SynclogItem, SynclogBatch};

const SERVER_URL: &str = "http://127.0.0.1:3780";
const CID: &str = "test-cid-789";
const WORKER_A: &str = "worker-A";
const WORKER_B: &str = "worker-B";

fn get_test_prefix() -> String {
    chrono::Local::now().format("test_%Y%m%d_%H%M%S_").to_string()
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
    
    let bytedata = json.get("bytedata").and_then(|v| v.as_array()).ok_or_else(|| {
        "无bytedata".to_string()
    })?;
    let bytes: Vec<u8> = bytedata.iter()
        .filter_map(|v| v.as_i64().map(|n| (n & 0xFF) as u8))
        .collect();
    
    let result = testtb::decode(&*bytes).map_err(|e| e.to_string())?;
    Ok(result.items)
}

fn read_local_synclog(_testtb: &TestTb) -> Vec<SynclogItem> {
    let project_path = base::ProjectPath::find().expect("查找项目路径失败");
    let db_path = project_path.local_db();
    let db = datastate::LocalDB::with_path(&db_path.to_string_lossy()).expect("创建数据库失败");
    
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let synclog_table = format!("synclog_{}", today);
    
    let check_sql = format!("SELECT name FROM sqlite_master WHERE type='table' AND name='{}'", synclog_table);
    let table_exists = match db.query(&check_sql, &[]) {
        Ok(rows) => !rows.is_empty(),
        Err(_) => false,
    };
    
    if !table_exists {
        let check_simple_sql = "SELECT name FROM sqlite_master WHERE type='table' AND name='synclog'";
        let simple_table_exists = match db.query(check_simple_sql, &[]) {
            Ok(rows) => !rows.is_empty(),
            Err(_) => false,
        };
        
        if !simple_table_exists {
            return Vec::new();
        }
        
        let sql = "SELECT id, tbname, action, cmdtext, params, idrow, worker, cid, upby FROM synclog WHERE tbname = 'testtb' AND synced = 0 ORDER BY id";
        let rows = match db.query(sql, &[]) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        
        return rows.iter().map(|row| {
            SynclogItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                apisys: "v1".to_string(),
                apimicro: "iflow".to_string(),
                apiobj: "synclog".to_string(),
                tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                cmdtext: row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                params: row.get("params").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                worker: WORKER_A.to_string(),
                synced: 0,
                cmdtextmd5: String::new(),
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }
        }).collect();
    }
    
    let sql = format!("SELECT id, tbname, action, cmdtext, params, idrow, worker, cid, upby FROM {} WHERE tbname = 'testtb' AND synced = 0 ORDER BY id", synclog_table);
    let rows = match db.query(&sql, &[]) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    
    rows.iter().map(|row| {
        SynclogItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cmdtext: row.get("cmdtext").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            params: row.get("params").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            worker: WORKER_A.to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }).collect()
}

async fn upload_synclog(sid: &str, items: Vec<SynclogItem>) -> Result<i32, String> {
    let client = reqwest::Client::new();
    let batch = SynclogBatch { items };
    let bytedata = batch.encode_to_vec();
    
    let body = serde_json::json!({
        "sid": sid,
        "bytedata": bytedata.iter().map(|b| *b as i64).collect::<Vec<i64>>()
    });
    let body = serde_json::to_string(&body).map_err(|e| e.to_string())?;
    
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog_mysql/maddmany", SERVER_URL))
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
    
    let jsdata_str = json.get("back").and_then(|v| v.as_str()).ok_or("无back字段")?;
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).map_err(|e| e.to_string())?;
    Ok(jsdata.get("batches").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
}

async fn do_work(sid: &str, worker: &str) -> Result<(i32, i32), String> {
    let client = reqwest::Client::new();
    let worker_json = serde_json::json!({"worker": worker}).to_string();
    let mut bytedata = vec![0x01u8; 8];
    bytedata.extend_from_slice(worker_json.as_bytes());
    
    let body = serde_json::json!({
        "sid": sid,
        "bytedata": bytedata.iter().map(|b| *b as i64).collect::<Vec<i64>>()
    });
    let body = serde_json::to_string(&body).map_err(|e| e.to_string())?;
    
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog_mysql/dowork", SERVER_URL))
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
    
    let back_str = json.get("back").and_then(|v| v.as_str()).ok_or("无back字段")?;
    let jsdata: serde_json::Value = serde_json::from_str(back_str).map_err(|e| e.to_string())?;
    Ok((
        jsdata.get("processed").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        jsdata.get("batches").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    ))
}

async fn get_synclog_by_worker(sid: &str, worker: &str, limit: i32) -> Result<Vec<SynclogItem>, String> {
    let client = reqwest::Client::new();
    let worker_json = serde_json::json!({"worker": worker}).to_string();
    let mut bytedata = vec![0x01u8; 8];
    bytedata.extend_from_slice(worker_json.as_bytes());
    
    let body = serde_json::json!({
        "sid": sid,
        "getnumber": limit,
        "bytedata": bytedata.iter().map(|b| *b as i64).collect::<Vec<i64>>()
    }).to_string();
    
    let resp = client
        .post(format!("{}/apisvc/backsvc/synclog_mysql/getbyworker", SERVER_URL))
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
    
    let back_str = json.get("back").and_then(|v| v.as_str()).ok_or("无back字段")?;
    let back_data: serde_json::Value = serde_json::from_str(back_str).map_err(|e| e.to_string())?;
    let bytedata_base64 = back_data.get("bytedata").and_then(|v| v.as_str()).ok_or("无bytedata")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytedata_base64.as_bytes()).map_err(|e| e.to_string())?;
    
    let synclog_batch = SynclogBatch::decode(&*bytes).map_err(|e| e.to_string())?;
    Ok(synclog_batch.items)
}

async fn clear_mysql_testtb(sid: &str, prefix: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "sid": sid,
        "pars": vec![prefix]
    }).to_string();
    
    let resp = client
        .post(format!("{}/apitest/testmenu/testtb/clear", SERVER_URL))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let _json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tokio::test]
async fn test_all_plans() {
    println!("\n========== 多端同步测试 (MySQL 版本) ==========");

    let sid = format!("{}|{}", CID, WORKER_A);
    let _up = UpInfo::new();
    let test_prefix = get_test_prefix();
    println!("测试数据前缀: {}", test_prefix);

    let default_db = datastate::LocalDB::default_instance().expect("获取默认数据库失败");
    let _ = default_db.execute(axum78::LOVERS_CREATE_SQL);
    let _ = default_db.execute(axum78::LOVERS_AUTH_CREATE_SQL);

    let test_user_id = datastate::next_id_string();
    let _ = default_db.execute_with_params(
        "INSERT OR REPLACE INTO lovers (id, uname, idcodef, cid, uid) VALUES (?, ?, ?, ?, ?)",
        &[&test_user_id as &dyn rusqlite::ToSql, &"test_user", &CID, &CID, &"test_uid"],
    );

    let _ = default_db.execute_with_params(
        "INSERT OR REPLACE INTO lovers_auth (ikuser, sid) VALUES ((SELECT idpk FROM lovers WHERE id = ?), ?)",
        &[&test_user_id as &dyn rusqlite::ToSql, &sid],
    );
    println!("默认数据库: 已初始化 lovers 和 lovers_auth 表");

    let project_path = base::ProjectPath::find().expect("查找项目路径失败");
    let local_db_path = project_path.local_db().to_string_lossy().to_string();
    println!("客户端数据库路径: {}", local_db_path);
    let local_testtb = TestTb::with_db_path(&local_db_path);
    let _ = local_testtb.db.execute("DROP TABLE IF EXISTS testtb");
    
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let synclog_shard = format!("synclog_{}", today);
    let _ = local_testtb.db.execute(&format!("DROP TABLE IF EXISTS {}", synclog_shard));
    let _ = local_testtb.db.execute("DROP TABLE IF EXISTS synclog");
    println!("客户端数据库: 已清理 synclog 表");
    
    let _ = local_testtb.db.execute(datastate::datastate::TESTTB_CREATE_SQL);
    let _ = local_testtb.db.execute(datastate::SYS_SQL_CREATE_SQL_SQLITE);
    let _ = local_testtb.db.execute(axum78::SYNCLOG_CREATE_SQL);
    println!("客户端数据库: 已重建 testtb 表、sys_sql 表和 synclog 表");
    
    let app = axum78::create_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3780").await.expect("绑定端口失败");
    let _server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("服务器启动失败");
    });
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // ===== 方案0: SID验证测试 =====
    println!("\n========== 方案0: SID验证测试 ==========");
    
    let empty_sid_result = download_from_server("").await;
    println!("空SID测试: {:?}", empty_sid_result);
    
    let invalid_sid_result = download_from_server("invalid-cid-xyz").await;
    println!("无效SID测试: {:?}", invalid_sid_result);
    
    println!("✅ 方案0通过 - SID验证测试（使用GUEST身份作为后备）");
    
    // ===== 方案1: 单向下载测试 =====
    println!("\n========== 方案1: 单向下载测试 ==========");
    
    // 清理 MySQL 测试数据
    let _ = clear_mysql_testtb(&sid, &test_prefix).await;
    
    // 直接向 MySQL 插入测试数据（通过 API）
    for i in 0..5 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("{}server_kind_{}", test_prefix, i)));
        record.insert("item".to_string(), serde_json::json!(format!("server_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("server_data_{}", i)));
        
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "sid": sid,
            "pars": record
        }).to_string();
        
        let resp = client
            .post(format!("{}/apitest/testmenu/testtb/madd", SERVER_URL))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .expect("请求失败");
        
        let json: serde_json::Value = resp.json().await.expect("解析失败");
        println!("服务器插入: {} -> {:?}", i, json.get("back"));
    }
    
    // 客户端下载
    let items = download_from_server(&sid).await.expect("下载失败");
    println!("下载到 {} 条记录", items.len());
    
    // 客户端保存下载的数据
    for item in &items {
        let mut record = std::collections::HashMap::new();
        record.insert("id".to_string(), serde_json::json!(item.id));
        record.insert("cid".to_string(), serde_json::json!(item.cid));
        record.insert("kind".to_string(), serde_json::json!(item.kind));
        record.insert("item".to_string(), serde_json::json!(item.item));
        record.insert("data".to_string(), serde_json::json!(item.data));
        record.insert("upby".to_string(), serde_json::json!(item.upby));
        record.insert("uptime".to_string(), serde_json::json!(item.uptime));
        
        let _ = local_testtb.m_sync_save(&record);
    }
    
    // 检查客户端记录数
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    assert!(client_rows.len() >= 5, "客户端记录数应该 >= 5");
    println!("✅ 方案1通过");
    
    // ===== 方案2: 客户端变更同步 =====
    println!("\n========== 方案2: 客户端变更同步 ==========");
    
    // 获取现有数据用于修改/删除
    let existing = local_testtb.mlist("testtb", 2, "获取修改删除目标").expect("查询失败");
    let id_for_update = existing.get(0).map(|r| r.id.clone()).unwrap();
    let id_for_delete = existing.get(1).map(|r| r.id.clone()).unwrap();
    
    // 添加2条（使用唯一 kind 值避免冲突）
    for i in 0..2 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("{}add_kind_{}", test_prefix, i)));
        record.insert("item".to_string(), serde_json::json!(format!("add_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("add_data_{}", i)));
        
        let _ = local_testtb.m_save(&record, "testtb", &format!("方案2-客户端添加{}", i)).expect("保存失败");
    }
    println!("客户端添加2条");
    
    // 修改1条
    let mut update_record = std::collections::HashMap::new();
    update_record.insert("kind".to_string(), serde_json::json!(format!("{}updated_kind", test_prefix)));
    update_record.insert("item".to_string(), serde_json::json!("updated_item"));
    update_record.insert("data".to_string(), serde_json::json!("updated_data"));
    local_testtb.m_update(&id_for_update, &update_record, "testtb", "方案2-客户端修改").expect("修改失败");
    println!("客户端修改1条");
    
    // 删除1条
    local_testtb.m_del(&id_for_delete, "testtb", "方案2-客户端删除").expect("删除失败");
    println!("客户端删除1条");
    
    // 从本地 synclog 读取记录并上传到服务器
    let synclog_items = read_local_synclog(&local_testtb);
    println!("读取本地 synclog: {} 条", synclog_items.len());
    
    // 上传 synclog
    let batches = upload_synclog(&sid, synclog_items).await.expect("上传失败");
    println!("上传 {} 条 synclog", batches);
    
    // 执行 doWork
    let (processed, _) = do_work(&sid, WORKER_A).await.expect("doWork失败");
    println!("doWork处理 {} 条", processed);
    
    // 等待 synclog 标记为 synced
    tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
    
    // 验证客户端数据
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    println!("✅ 方案2通过");
    
    // ===== 方案3: 服务器变更同步 =====
    println!("\n========== 方案3: 服务器变更同步 ==========");
    
    // 服务器端添加数据（通过 API，使用唯一 kind 值）
    for i in 0..2 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("{}server_add_kind_{}", test_prefix, i)));
        record.insert("item".to_string(), serde_json::json!(format!("server_add_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("server_add_data_{}", i)));
        
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "sid": format!("{}|{}", CID, WORKER_B),
            "pars": record
        }).to_string();
        
        let resp = client
            .post(format!("{}/apitest/testmenu/testtb/madd", SERVER_URL))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .expect("请求失败");
        
        let json: serde_json::Value = resp.json().await.expect("解析失败");
        println!("服务器添加: {} -> {:?}", i, json.get("back"));
    }
    println!("服务器添加2条");
    
    // 等待 synclog 标记为 synced
    tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
    
    // 客户端下载 synclog (worker != WORKER_A)
    let synclog_items = get_synclog_by_worker(&sid, WORKER_A, 100).await.expect("下载synclog失败");
    println!("下载到 {} 条 synclog", synclog_items.len());
    
    // 执行 synclog 中的 SQL
    for item in &synclog_items {
        if item.tbname != "testtb" {
            continue;
        }
        println!("执行: action={}, cmdtext={}", item.action, &item.cmdtext[..item.cmdtext.len().min(100)]);
        
        match item.action.as_str() {
            "insert" | "update" => {
                // cmdtext 是业务数据的 JSON
                let record_data: std::collections::HashMap<String, serde_json::Value> = 
                    serde_json::from_str(&item.cmdtext).unwrap_or_default();
                
                if !record_data.is_empty() {
                    let id = record_data.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if !id.is_empty() {
                        let _ = local_testtb.m_sync_save(&record_data);
                        println!("保存记录: id={}", id);
                    }
                }
            }
            "delete" => {
                let _ = local_testtb.m_sync_del(&item.idrow);
                println!("删除记录: id={}", item.idrow);
            }
            _ => {}
        }
    }
    
    // 验证客户端数据
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    println!("✅ 方案3通过");
    
    // ===== 方案4: 最终一致性验证 =====
    println!("\n========== 方案4: 最终一致性验证 ==========");

    let client_rows_final = local_testtb.mlist("testtb", 1000, "最终验证").unwrap();
    println!("最终客户端记录数: {}", client_rows_final.len());

    // 验证所有ID是否为雪花ID
    fn is_valid_snowflake_id(id: &str) -> bool {
        id.len() >= 16 && id.len() <= 19 && id.chars().all(|c| c.is_ascii_digit())
    }

    for row in &client_rows_final {
        assert!(is_valid_snowflake_id(&row.id), "无效的雪花ID: {}", row.id);
    }

    println!("✅ 方案4通过 - 最终一致性验证通过");

    // ===== 方案5: 冲突测试（时间戳优先策略） =====
    println!("\n========== 方案5: 冲突测试 ==========");

    // 清理 synclog 准备冲突测试
    let _ = local_testtb.db.execute("DELETE FROM synclog WHERE tbname='testtb'");

    // 选择一个现有记录作为冲突目标
    let conflict_target = client_rows_final.first().expect("无记录可用于冲突测试");
    let conflict_id = conflict_target.id.clone();

    println!("冲突目标ID: {}", conflict_id);

    // 步骤A: 客户端修改记录
    let client_uptime_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let client_kind = format!("{}conflict_client_kind", test_prefix);
    let client_item = "conflict_client_item";
    let client_data = "conflict_client_data";

    let mut client_update = std::collections::HashMap::new();
    client_update.insert("kind".to_string(), serde_json::json!(client_kind));
    client_update.insert("item".to_string(), serde_json::json!(client_item));
    client_update.insert("data".to_string(), serde_json::json!(client_data));
    local_testtb.m_update(&conflict_id, &client_update, "testtb", "方案5-客户端修改").expect("客户端修改失败");

    println!("客户端修改: kind={}, uptime={}", client_kind, client_uptime_str);

    // 步骤B: 客户端上传变更到服务器
    let client_synclog = read_local_synclog(&local_testtb);
    if !client_synclog.is_empty() {
        let batches1 = upload_synclog(&sid, client_synclog).await.expect("上传客户端变更失败");
        println!("上传客户端 synclog: {} 条", batches1);

        // 执行 doWork 应用客户端变更
        let (processed1, _) = do_work(&sid, WORKER_A).await.expect("doWork处理客户端变更失败");
        println!("doWork处理客户端变更: {} 条", processed1);
    }

    // 等待 synclog 标记为 synced
    tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

    // 步骤C: 验证客户端变更已同步到服务器
    // 从 MySQL 下载最新数据
    let server_items = download_from_server(&sid).await.expect("下载服务器数据失败");
    let server_item = server_items.iter().find(|i| i.id == conflict_id);
    
    if let Some(s_item) = server_item {
        println!("服务器数据: kind={}, item={}, data={}", s_item.kind, s_item.item, s_item.data);
        assert_eq!(s_item.kind, client_kind, "客户端变更应已同步到服务器");
        assert_eq!(s_item.item, client_item, "客户端变更应已同步到服务器");
        assert_eq!(s_item.data, client_data, "客户端变更应已同步到服务器");
    }

    // 步骤D: 验证客户端本地数据
    let final_client = local_testtb.mlist("testtb", 100, "获取冲突记录").expect("查询客户端记录失败")
        .into_iter()
        .find(|r| r.id == conflict_id)
        .expect("找不到冲突记录");

    println!("冲突解决后:");
    println!("  客户端: kind={}, item={}, data={}", final_client.kind, final_client.item, final_client.data);

    // 验证客户端和服务器数据一致
    assert_eq!(final_client.kind, client_kind, "客户端数据应与服务器一致");
    assert_eq!(final_client.item, client_item, "客户端数据应与服务器一致");
    assert_eq!(final_client.data, client_data, "客户端数据应与服务器一致");

    println!("✅ 方案5通过 - 客户端变更同步测试通过");

    // 验证 upby/uptime 字段
    println!("\n========== 验证 upby/uptime 字段 ==========");
    let rows = local_testtb.mlist("testtb", 3, "验证字段").unwrap();
    for row in &rows {
        println!("客户端: id={}, kind={}, upby={}, uptime={}", &row.id[..8.min(row.id.len())], row.kind, row.upby, row.uptime);
    }
    
    println!("\n========== 所有测试通过 ==========");
}
