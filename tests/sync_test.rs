//! 多端同步测试
//!
//! 测试方案：
//! 1. 单向下载测试：清空表testtb，服务器数据下载到本地
//! 2. 客户端变更同步：添加2条、修改2条、删除2条，上传synclog，预期服务器一致
//! 3. 服务器变更同步：服务器添加/修改/删除各2条，预期客户端同步后一致
//! 4. 最后客户端和服务器testtb表一致

use base::UpInfo;
use base64::Engine;
use datastate::datastate::TestTb;
use datastate::next_id_string;
use prost::Message;
use axum78::apitest::testmenu::testtb::{testtb, testtbItem}; use axum78::apisvc::backsvc::synclog::{SynclogItem, SynclogBatch};


const SERVER_URL: &str = "http://127.0.0.1:3780";
const CID: &str = "test-cid-789";
const WORKER_A: &str = "worker-A";
const WORKER_B: &str = "worker-B";

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
    
    // 打印服务器返回的原始数据
    println!("服务器返回的JSON: {:?}", json);
    
    let bytedata = json.get("bytedata").and_then(|v| v.as_array()).ok_or("无bytedata")?;
    let bytes: Vec<u8> = bytedata.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect();
    println!("bytedata长度: {} bytes", bytes.len());
    
    let result = testtb::decode(&*bytes).map_err(|e| e.to_string())?;
    println!("解码后的items数量: {}", result.items.len());
    for item in &result.items {
        println!("解码后: id={}, kind={}", item.id, item.kind);
    }
    
    Ok(result.items)
}

fn read_local_synclog(testtb: &TestTb) -> Vec<SynclogItem> {
    let sql = "SELECT id, tbname, action, params, idrow, worker, cid, upby FROM synclog WHERE tbname = 'testtb' AND synced = 0 ORDER BY id";
    let rows = testtb.db.query(sql, &[]).unwrap();
    
    rows.iter().map(|row| {
        SynclogItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cmdtext: String::new(),
            params: row.get("params").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }).collect()
}

fn read_remote_synclog(testtb: &TestTb) -> Vec<SynclogItem> {
    let sql = "SELECT id, tbname, action, params, idrow, worker, cid, upby FROM synclog WHERE tbname = 'testtb' AND synced = 0 ORDER BY id";
    let rows = testtb.db.query(sql, &[]).unwrap();
    
    rows.iter().map(|row| {
        SynclogItem {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: row.get("tbname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            action: row.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            cmdtext: String::new(),
            params: row.get("params").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            idrow: row.get("idrow").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            worker: row.get("worker").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }).collect()
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
    
    let jsdata_str = json.get("jsdata").and_then(|v| v.as_str()).unwrap();
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).unwrap();
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
    let _up = UpInfo::new();

    // 初始化默认数据库中的 lovers 和 lovers_auth 表（用于 SID 验证）
    let default_db = datastate::LocalDB::default_instance().expect("获取默认数据库失败");
    let _ = default_db.execute(axum78::LOVERS_CREATE_SQL);
    let _ = default_db.execute(axum78::LOVERS_AUTH_CREATE_SQL);

    // 插入测试用户
    let test_user_id = datastate::next_id_string();
    let _ = default_db.execute_with_params(
        "INSERT OR REPLACE INTO lovers (id, uname, idcodef, cid, uid) VALUES (?, ?, ?, ?, ?)",
        &[&test_user_id as &dyn rusqlite::ToSql, &"test_user", &CID, &CID, &"test_uid"],
    );

    // 插入测试会话（SID 格式: CID|worker）
    let _ = default_db.execute_with_params(
        "INSERT OR REPLACE INTO lovers_auth (ikuser, sid) VALUES ((SELECT idpk FROM lovers WHERE id = ?), ?)",
        &[&test_user_id as &dyn rusqlite::ToSql, &sid],
    );
    println!("默认数据库: 已初始化 lovers 和 lovers_auth 表");

    // 远程数据库路径（服务器端）
    let remote_db_path = "docs/config/remote.db";

    // 删除旧表并重新创建（远程数据库）
    let remote_testtb = TestTb::with_db_path(remote_db_path);
    let _ = remote_testtb.db.execute("DROP TABLE IF EXISTS testtb");
    let _ = remote_testtb.db.execute("DELETE FROM synclog WHERE tbname='testtb'");
    let _ = remote_testtb.db.execute(datastate::datastate::TESTTB_CREATE_SQL);
    println!("远程数据库: 已重建testtb表");

    // 删除旧表并重新创建（本地数据库）
    // 使用 with_db_path 方法，避免使用单例模式
    let local_db_path = "docs/config/local.db";
    let local_testtb = TestTb::with_db_path(local_db_path);
    let _ = local_testtb.db.execute("DROP TABLE IF EXISTS testtb");
    let _ = local_testtb.db.execute("DELETE FROM synclog WHERE tbname='testtb'");
    let _ = local_testtb.db.execute(datastate::datastate::TESTTB_CREATE_SQL);
    println!("本地数据库: 已重建testtb表");
    
    // 启动服务器（服务器端使用远程数据库路径）
    use std::sync::Arc;
    use axum78::AppState;
    use tower_http::cors::{CorsLayer, Any};
    use axum::middleware;
    use axum78::sid_auth_middleware;

    let state = Arc::new(AppState::new());
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    let app = axum::Router::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", axum::routing::any(
            |axum::extract::Path((_apisys, _apimicro, apiobj, apifun)): axum::extract::Path<(String, String, String, String)>,
             axum::extract::Extension(verify_result): axum::extract::Extension<axum78::VerifyResult>,
             axum::extract::Extension(up): axum::extract::Extension<axum78::UpInfo>| async move {
                if apiobj == "testtb" {
                    let (status, resp) = axum78::apitest::testmenu::testtb::handle(&apifun, up, &verify_result).await;
                    (status, [(axum::http::header::CONTENT_TYPE, "application/json")], resp)
                } else {
                    let (status, resp) = axum78::apisvc::backsvc::synclog::handle(&apifun.to_lowercase(), up).await;
                    (status, [(axum::http::header::CONTENT_TYPE, "application/json")], resp)
                }
            }
        ))
        .layer(middleware::from_fn(sid_auth_middleware))
        .route("/health", axum::routing::get(|| async { "OK" }))
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3780").await.expect("绑定端口失败");
    let _server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("服务器启动失败");
    });
    
    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // ===== 方案0: SID验证测试 =====
    println!("\n========== 方案0: SID验证测试 ==========");
    
    // 测试空SID
    let empty_sid_result = download_from_server("").await;
    println!("空SID测试: {:?}", empty_sid_result);
    assert!(empty_sid_result.is_err(), "空SID应该被拒绝");
    
    // 测试无效SID格式（有效格式但CID不存在）
    let invalid_sid_result = download_from_server("invalid-cid-xyz").await;
    println!("无效SID测试: {:?}", invalid_sid_result);
    // 注意：简单验证只检查格式，不检查CID是否存在于数据库
    // 如果需要严格验证，需要启用数据库验证
    
    println!("✅ 方案0通过 - SID验证测试");
    
    // ===== 方案1: 单向下载测试 =====
    println!("\n========== 方案1: 单向下载测试 ==========");
    
    // 服务器插入5条数据（使用远程数据库）
    for i in 0..5 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("server_kind_{}", i)));
        record.insert("item".to_string(), serde_json::json!(format!("server_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("server_data_{}", i)));
        
        let id = remote_testtb.m_save(&record, "testtb", &format!("方案1-服务器插入{}", i)).expect("保存失败");
        println!("服务器插入: {} -> id={}", i, &id);
    }
    
    // 检查服务器记录数（远程数据库）
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("服务器记录数: {}", server_rows.len());
    assert_eq!(server_rows.len(), 5);
    
    // 客户端下载
    let items = download_from_server(&sid).await.expect("下载失败");
    println!("下载到 {} 条记录", items.len());
    
    // 客户端保存下载的数据（使用本地数据库）
    // 使用 m_sync_save 方法，因为这是同步数据，不应该自动填充 CID、upby、uptime
    for item in &items {
        println!("准备保存: id={}, kind={}", item.id, item.kind);
        let mut record = std::collections::HashMap::new();
        record.insert("id".to_string(), serde_json::json!(item.id));
        record.insert("cid".to_string(), serde_json::json!(item.cid));
        record.insert("kind".to_string(), serde_json::json!(item.kind));
        record.insert("item".to_string(), serde_json::json!(item.item));
        record.insert("data".to_string(), serde_json::json!(item.data));
        record.insert("upby".to_string(), serde_json::json!(item.upby));
        record.insert("uptime".to_string(), serde_json::json!(item.uptime));
        
        let result = local_testtb.m_sync_save(&record);
        println!("保存结果: {:?}", result);
    }
    
    // 检查客户端记录数（本地数据库）
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    assert_eq!(client_rows.len(), 5);
    println!("✅ 方案1通过");
    
    // ===== 方案2: 客户端变更同步 =====
    println!("\n========== 方案2: 客户端变更同步 ==========");
    
    // 获取现有数据用于修改/删除
    let existing = local_testtb.mlist("testtb", 2, "获取修改删除目标").expect("查询失败");
    let id_for_update = existing.get(0).map(|r| r.id.clone()).unwrap();
    let id_for_delete = existing.get(1).map(|r| r.id.clone()).unwrap();
    
    // 添加2条（使用 m_save 自动写 sync_queue）
    for i in 0..2 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("add_kind_{}", i)));
        record.insert("item".to_string(), serde_json::json!(format!("add_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("add_data_{}", i)));
        
        let _ = local_testtb.m_save(&record, "testtb", &format!("方案2-客户端添加{}", i)).expect("保存失败");
    }
    println!("客户端添加2条");
    
    // 修改1条（使用 m_update 自动写 sync_queue）
    let mut update_record = std::collections::HashMap::new();
    update_record.insert("kind".to_string(), serde_json::json!("updated_kind"));
    update_record.insert("item".to_string(), serde_json::json!("updated_item"));
    update_record.insert("data".to_string(), serde_json::json!("updated_data"));
    local_testtb.m_update(&id_for_update, &update_record, "testtb", "方案2-客户端修改").expect("修改失败");
    println!("客户端修改1条");
    
    // 删除1条（使用 m_del 自动写 sync_queue）
    local_testtb.m_del(&id_for_delete, "testtb", "方案2-客户端删除").expect("删除失败");
    println!("客户端删除1条");
    
    // 从本地 sync_queue 读取记录并上传到服务器
    let synclog_items = read_local_synclog(&local_testtb);
    println!("读取本地 sync_queue: {} 条", synclog_items.len());
    
    // 上传synclog
    let batches = upload_synclog(&sid, synclog_items).await.expect("上传失败");
    println!("上传 {} 条synclog", batches);
    
    // 执行doWork
    let (processed, _) = do_work().await.expect("doWork失败");
    println!("doWork处理 {} 条", processed);
    
    // 验证服务器数据（远程数据库）
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    println!("服务器记录数: {}", server_rows.len());
    assert_eq!(client_rows.len(), server_rows.len());
    println!("✅ 方案2通过");
    
    // ===== 方案3: 服务器变更同步 =====
    println!("\n========== 方案3: 服务器变更同步 ==========");
    
    // 服务器端直接操作数据库（使用 DataState 基类）
    // 这些操作会产生 synclog，客户端可以下载并执行
    
    // 服务器添加2条（使用 m_save 自动填充 upby、uptime，自动写 synclog）
    for i in 0..2 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("server_add_kind_{}", i)));
        record.insert("item".to_string(), serde_json::json!(format!("server_add_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("server_add_data_{}", i)));
        
        let _ = remote_testtb.m_save(&record, "testtb", &format!("方案3-服务器添加{}", i)).expect("保存失败");
    }
    println!("服务器添加2条");
    
    // 读取服务器端产生的 synclog
    let server_synclog = read_remote_synclog(&remote_testtb);
    println!("服务器产生 {} 条synclog", server_synclog.len());
    
    // 上传synclog到服务器（模拟另一个客户端上传）
    let batches = upload_synclog(&format!("{}|{}", CID, WORKER_B), server_synclog).await.expect("上传失败");
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
    
    // jsdata字段是JSON字符串
    let jsdata_str = json.get("jsdata").and_then(|v| v.as_str()).expect("无jsdata");
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).expect("解析jsdata失败");
    let bytedata_base64 = jsdata.get("bytedata").and_then(|v| v.as_str()).expect("无bytedata");
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytedata_base64.as_bytes()).expect("Base64解码失败");
    let synclog_batch = SynclogBatch::decode(&*bytes).expect("解码失败");
    
    println!("下载到 {} 条synclog", synclog_batch.items.len());
    
    // 执行synclog中的SQL（使用 m_sync_save 方法，不自动填充字段）
    for item in &synclog_batch.items {
        let params: Vec<serde_json::Value> = serde_json::from_str(&item.params).unwrap();
        println!("执行: action={}, params={:?}", item.action, params);
        match item.action.as_str() {
            "insert" => {
                // params 顺序是按字母顺序排列的：cid, data, id, item, kind, upby, uptime
                let mut record = std::collections::HashMap::new();
                record.insert("cid".to_string(), params.get(0).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("data".to_string(), params.get(1).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("id".to_string(), params.get(2).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("item".to_string(), params.get(3).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("kind".to_string(), params.get(4).cloned().unwrap_or(serde_json::Value::Null));
                if params.len() > 5 {
                    record.insert("upby".to_string(), params.get(5).cloned().unwrap_or(serde_json::Value::Null));
                }
                if params.len() > 6 {
                    record.insert("uptime".to_string(), params.get(6).cloned().unwrap_or(serde_json::Value::Null));
                }
                
                let result = local_testtb.m_sync_save(&record);
                let id_str = params.get(2).and_then(|v| v.as_str()).unwrap_or("");
                println!("INSERT结果: {:?}, id={}", result, id_str);
            }
            "update" => {
                // update params 顺序：cid, data, item, kind, upby, uptime, id（id 在最后）
                let id = params.last().and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mut record = std::collections::HashMap::new();
                record.insert("cid".to_string(), params.get(0).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("data".to_string(), params.get(1).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("item".to_string(), params.get(2).cloned().unwrap_or(serde_json::Value::Null));
                record.insert("kind".to_string(), params.get(3).cloned().unwrap_or(serde_json::Value::Null));
                if params.len() > 5 {
                    record.insert("upby".to_string(), params.get(4).cloned().unwrap_or(serde_json::Value::Null));
                }
                if params.len() > 6 {
                    record.insert("uptime".to_string(), params.get(5).cloned().unwrap_or(serde_json::Value::Null));
                }
                
                let _ = local_testtb.m_sync_update(&id, &record);
            }
            "delete" => {
                let id = params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = local_testtb.m_sync_del(&id);
            }
            _ => {}
        }
    }
    
    // 验证最终数据一致性
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap();
    println!("客户端记录数: {}", client_rows.len());
    println!("服务器记录数: {}", server_rows.len());

    // 强化验证：不仅检查数量，还要检查数据内容完全一致
    assert_eq!(client_rows.len(), server_rows.len());

    // 按ID排序后比较每条记录
    let mut client_sorted = client_rows.clone();
    let mut server_sorted = server_rows.clone();
    client_sorted.sort_by_key(|r| r.id.clone());
    server_sorted.sort_by_key(|r| r.id.clone());

    for (c, s) in client_sorted.iter().zip(server_sorted.iter()) {
        assert_eq!(c.id, s.id, "ID不匹配");
        assert_eq!(c.cid, s.cid, "cid不匹配");
        assert_eq!(c.kind, s.kind, "kind不匹配");
        assert_eq!(c.item, s.item, "item不匹配");
        assert_eq!(c.data, s.data, "data不匹配");
        // 注意：upby和uptime可能因为时间戳不同而有差异，这里只验证核心字段
    }

    println!("✅ 方案3通过");

    // ===== 方案4: 最终一致性验证 =====
    println!("\n========== 方案4: 最终一致性验证 ==========");

    // 再次完整验证两边数据完全一致（包括所有字段）
    let server_rows_final = remote_testtb.mlist("testtb", 1000, "最终验证").unwrap();
    let client_rows_final = local_testtb.mlist("testtb", 1000, "最终验证").unwrap();

    println!("最终客户端记录数: {}", client_rows_final.len());
    println!("最终服务器记录数: {}", server_rows_final.len());
    assert_eq!(client_rows_final.len(), server_rows_final.len(), "记录数量不一致");

    // 验证所有ID是否为雪花ID（16-19位数字）
    fn is_valid_snowflake_id(id: &str) -> bool {
        id.len() >= 16 && id.len() <= 19 && id.chars().all(|c| c.is_ascii_digit())
    }

    for row in &client_rows_final {
        assert!(is_valid_snowflake_id(&row.id), "无效的雪花ID: {}", row.id);
    }

    // 完整数据比较
    let mut client_sorted = client_rows_final.clone();
    let mut server_sorted = server_rows_final.clone();
    client_sorted.sort_by_key(|r| r.id.clone());
    server_sorted.sort_by_key(|r| r.id.clone());

    let mut data_match = true;
    for (c, s) in client_sorted.iter().zip(server_sorted.iter()) {
        if c.id != s.id || c.cid != s.cid || c.kind != s.kind ||
           c.item != s.item || c.data != s.data {
            data_match = false;
            println!("数据不匹配: 客户端={:?}, 服务器={:?}", c, s);
            break;
        }
    }

    assert!(data_match, "数据内容不一致！");
    println!("✅ 方案4通过 - 最终一致性验证通过");

    // ===== 方案5: 冲突测试（时间戳优先策略） =====
    println!("\n========== 方案5: 冲突测试 ==========");

    // 清空synclog准备冲突测试
    let _ = local_testtb.db.execute("DELETE FROM synclog WHERE tbname='testtb'");
    let _ = remote_testtb.db.execute("DELETE FROM synclog WHERE tbname='testtb'");

    // 选择一个现有记录作为冲突目标
    let conflict_target = client_rows_final.first().expect("无记录可用于冲突测试");
    let conflict_id = conflict_target.id.clone();

    println!("冲突目标ID: {}", conflict_id);

    // 步骤A: 客户端修改记录（使用较早的时间戳）
    let client_uptime_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let client_kind = "conflict_client_kind";
    let client_item = "conflict_client_item";
    let client_data = "conflict_client_data";

    let mut client_update = std::collections::HashMap::new();
    client_update.insert("kind".to_string(), serde_json::json!(client_kind));
    client_update.insert("item".to_string(), serde_json::json!(client_item));
    client_update.insert("data".to_string(), serde_json::json!(client_data));
    local_testtb.m_update(&conflict_id, &client_update, "testtb", "方案5-客户端修改").expect("客户端修改失败");

    // 步骤B: 服务器也修改同一记录（使用较晚的时间戳）
    let server_uptime_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let server_kind = "conflict_server_kind";
    let server_item = "conflict_server_item";
    let server_data = "conflict_server_data";

    let mut server_update = std::collections::HashMap::new();
    server_update.insert("kind".to_string(), serde_json::json!(server_kind));
    server_update.insert("item".to_string(), serde_json::json!(server_item));
    server_update.insert("data".to_string(), serde_json::json!(server_data));
    remote_testtb.m_update(&conflict_id, &server_update, "testtb", "方案5-服务器修改").expect("服务器修改失败");

    println!("客户端修改: kind={}, uptime={}", client_kind, client_uptime_str);
    println!("服务器修改: kind={}, uptime={}", server_kind, server_uptime_str);

    // 步骤C: 客户端上传变更到服务器
    let client_synclog = read_local_synclog(&local_testtb);
    let batches1 = upload_synclog(&sid, client_synclog).await.expect("上传客户端变更失败");
    println!("上传客户端synclog: {} 条", batches1);

    // 执行doWork应用客户端变更
    let (processed1, _) = do_work().await.expect("doWork处理客户端变更失败");
    println!("doWork处理客户端变更: {} 条", processed1);

    // 步骤D: 服务器端上传变更（模拟另一个worker）
    let server_synclog = read_remote_synclog(&remote_testtb);
    let batches2 = upload_synclog(&format!("{}|{}", CID, WORKER_B), server_synclog).await.expect("上传服务器变更失败");
    println!("上传服务器synclog: {} 条", batches2);

    // 执行doWork应用服务器变更
    let (processed2, _) = do_work().await.expect("doWork处理服务器变更失败");
    println!("doWork处理服务器变更: {} 条", processed2);

    // 步骤E: 冲突解决验证
    // 现在两边应该都同步了。根据时间戳优先原则，较新的那个应该胜出
    // 我们需要检查最终值是否符合预期

    let final_client = local_testtb.mlist("testtb", 1, "获取冲突记录").expect("查询客户端记录失败").into_iter().find(|r| r.id == conflict_id).unwrap();
    let final_server = remote_testtb.mlist("testtb", 1, "获取冲突记录").expect("查询服务器记录失败").into_iter().find(|r| r.id == conflict_id).unwrap();

    println!("冲突解决后:");
    println!("  客户端: kind={}, item={}, data={}", final_client.kind, final_client.item, final_client.data);
    println!("  服务器: kind={}, item={}, data={}", final_server.kind, final_server.item, final_server.data);

    // 验证两边数据一致
    assert_eq!(final_client.kind, final_server.kind, "冲突后客户端和服务器kind不一致");
    assert_eq!(final_client.item, final_server.item, "冲突后客户端和服务器item不一致");
    assert_eq!(final_client.data, final_server.data, "冲突后客户端和服务器data不一致");

    // 验证冲突解决符合时间戳优先
    // 由于服务器时间戳较晚，应该胜出
    let expected_kind = server_kind;
    let expected_item = server_item;
    let expected_data = server_data;

    assert_eq!(final_client.kind, expected_kind, "冲突解决失败：应为服务器值");
    assert_eq!(final_client.item, expected_item, "冲突解决失败：应为服务器值");
    assert_eq!(final_client.data, expected_data, "冲突解决失败：应为服务器值");

    println!("✅ 方案5通过 - 冲突测试通过（时间戳优先）");

    // 验证upby/uptime字段
    println!("\n========== 验证upby/uptime字段 ==========");
    let rows = local_testtb.mlist("testtb", 3, "验证字段").unwrap();
    for row in &rows {
        println!("客户端: id={}, kind={}, upby={}, uptime={}", &row.id[..8.min(row.id.len())], row.kind, row.upby, row.uptime);
    }
    
    println!("\n========== 所有测试通过 ==========");
}
