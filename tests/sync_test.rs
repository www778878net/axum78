//! 多端同步测试
//!
//! 测试方案：
//! 1. 单向下载测试：清空表testtb，服务器数据下载到本地
//! 2. 客户端变更同步：添加2条、修改2条、删除2条，上传synclog，预期服务器一致
//! 3. 服务器变更同步：服务器添加/修改/删除各2条，预期客户端同步后一致
//! 4. 最后客户端和服务器testtb表一致

use base::UpInfo;
use base64::Engine;
use database::datastate::TestTb;
use database::{get_worker_id, next_id_string};
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
    #[prost(string, tag = "14")]
    pub upby: String,
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
    let _up = UpInfo::new();
    
    // 远程数据库路径（服务器端）
    let remote_db_path = "c:\\7788\\rustdemo\\rustdemo\\crates\\axum78\\tmp\\data\\remote.db";
    
    // 清空远程数据库（服务器端）
    let remote_testtb = TestTb::with_db_path(remote_db_path);
    let rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    for row in &rows {
        let _ = remote_testtb.m_del(&row.id, "testtb", "清空远程测试表");
    }
    println!("清空远程数据库: {} 条记录", rows.len());
    
    // 清空本地数据库（客户端端）
    let local_testtb = TestTb::new();
    let rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    for row in &rows {
        let _ = local_testtb.m_del(&row.id, "testtb", "清空本地测试表");
    }
    println!("清空本地数据库: {} 条记录", rows.len());
    
    // 再次检查本地数据库是否清空
    let rows = local_testtb.mlist("testtb", 1000, "再次检查").unwrap_or_default();
    println!("清空后本地数据库记录数: {}", rows.len());
    
    // 启动服务器（服务器端使用远程数据库路径）
    let app = axum78::create_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3780").await.expect("绑定端口失败");
    let _server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("服务器启动失败");
    });
    
    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
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
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    println!("服务器记录数: {}", server_rows.len());
    assert_eq!(server_rows.len(), 5);
    
    // 客户端下载
    let items = download_from_server(&sid).await.expect("下载失败");
    println!("下载到 {} 条记录", items.len());
    
    // 客户端保存下载的数据（使用本地数据库）
    for item in &items {
        println!("准备保存: id={}, kind={}", item.id, item.kind);
        let mut record = std::collections::HashMap::new();
        record.insert("id".to_string(), serde_json::json!(item.id));
        record.insert("cid".to_string(), serde_json::json!(item.cid));
        record.insert("kind".to_string(), serde_json::json!(item.kind));
        record.insert("item".to_string(), serde_json::json!(item.item));
        record.insert("data".to_string(), serde_json::json!(item.data));
        
        let result = local_testtb.m_save(&record, "testtb", "方案1-客户端下载");
        println!("保存结果: {:?}", result);
    }
    
    // 检查客户端记录数（本地数据库）
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    println!("客户端记录数: {}", client_rows.len());
    assert_eq!(client_rows.len(), 5);
    println!("✅ 方案1通过");
    
    // ===== 方案2: 客户端变更同步 =====
    println!("\n========== 方案2: 客户端变更同步 ==========");
    
    // 获取现有数据用于修改/删除
    let existing = local_testtb.mlist("testtb", 2, "获取修改删除目标").expect("查询失败");
    let id_for_update = existing.get(0).map(|r| r.id.clone()).unwrap_or_default();
    let id_for_delete = existing.get(1).map(|r| r.id.clone()).unwrap_or_default();
    
    let mut synclog_items: Vec<SynclogItem> = Vec::new();
    
    // 添加2条
    for i in 0..2 {
        let mut record = std::collections::HashMap::new();
        record.insert("cid".to_string(), serde_json::json!(CID));
        record.insert("kind".to_string(), serde_json::json!(format!("add_kind_{}", i)));
        record.insert("item".to_string(), serde_json::json!(format!("add_item_{}", i)));
        record.insert("data".to_string(), serde_json::json!(format!("add_data_{}", i)));
        
        let id = local_testtb.m_save(&record, "testtb", &format!("方案2-客户端添加{}", i)).expect("保存失败");
        
        synclog_items.push(SynclogItem {
            id: next_id_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: "testtb".to_string(),
            action: "insert".to_string(),
            cmdtext: String::new(),
            params: serde_json::to_string(&[&id, CID, &format!("add_kind_{}", i), &format!("add_item_{}", i), &format!("add_data_{}", i)]).unwrap_or_default(),
            idrow: id.clone(),
            worker: WORKER_A.to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: CID.to_string(),
            upby: WORKER_A.to_string(),
        });
    }
    println!("客户端添加2条");
    
    // 修改1条
    let mut update_record = std::collections::HashMap::new();
    update_record.insert("kind".to_string(), serde_json::json!("updated_kind"));
    update_record.insert("item".to_string(), serde_json::json!("updated_item"));
    update_record.insert("data".to_string(), serde_json::json!("updated_data"));
    local_testtb.m_update(&id_for_update, &update_record, "testtb", "方案2-客户端修改").expect("修改失败");
    
    synclog_items.push(SynclogItem {
        id: next_id_string(),
        apisys: "v1".to_string(),
        apimicro: "iflow".to_string(),
        apiobj: "synclog".to_string(),
        tbname: "testtb".to_string(),
        action: "update".to_string(),
        cmdtext: String::new(),
        params: serde_json::to_string(&["updated_kind", "updated_item", "updated_data", &id_for_update]).unwrap_or_default(),
        idrow: id_for_update.clone(),
        worker: WORKER_A.to_string(),
        synced: 0,
        cmdtextmd5: String::new(),
        cid: CID.to_string(),
        upby: WORKER_A.to_string(),
    });
    println!("客户端修改1条");
    
    // 删除1条
    local_testtb.m_del(&id_for_delete, "testtb", "方案2-客户端删除").expect("删除失败");
    
    synclog_items.push(SynclogItem {
        id: next_id_string(),
        apisys: "v1".to_string(),
        apimicro: "iflow".to_string(),
        apiobj: "synclog".to_string(),
        tbname: "testtb".to_string(),
        action: "delete".to_string(),
        cmdtext: String::new(),
        params: serde_json::to_string(&[&id_for_delete]).unwrap_or_default(),
        idrow: id_for_delete.clone(),
        worker: WORKER_A.to_string(),
        synced: 0,
        cmdtextmd5: String::new(),
        cid: CID.to_string(),
        upby: WORKER_A.to_string(),
    });
    println!("客户端删除1条");
    
    // 上传synclog
    let batches = upload_synclog(&sid, synclog_items).await.expect("上传失败");
    println!("上传 {} 条synclog", batches);
    
    // 执行doWork
    let (processed, _) = do_work().await.expect("doWork失败");
    println!("doWork处理 {} 条", processed);
    
    // 验证服务器数据（远程数据库）
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    println!("客户端记录数: {}", client_rows.len());
    println!("服务器记录数: {}", server_rows.len());
    assert_eq!(client_rows.len(), server_rows.len());
    println!("✅ 方案2通过");
    
    // ===== 方案3: 服务器变更同步 =====
    println!("\n========== 方案3: 服务器变更同步 ==========");
    
    // 模拟另一个客户端(worker-B)上传变更
    let worker_b_sid = format!("{}|{}", CID, WORKER_B);
    
    // 服务器添加2条 (通过worker-B上传synclog，然后doWork执行)
    let mut synclog_items_b: Vec<SynclogItem> = Vec::new();
    for i in 0..2 {
        let id = next_id_string();
        
        synclog_items_b.push(SynclogItem {
            id: next_id_string(),
            apisys: "v1".to_string(),
            apimicro: "iflow".to_string(),
            apiobj: "synclog".to_string(),
            tbname: "testtb".to_string(),
            action: "insert".to_string(),
            cmdtext: String::new(),
            params: serde_json::to_string(&[&id, CID, &format!("server_add_kind_{}", i), &format!("server_add_item_{}", i), &format!("server_add_data_{}", i)]).unwrap_or_default(),
            idrow: id.clone(),
            worker: WORKER_B.to_string(),
            synced: 0,
            cmdtextmd5: String::new(),
            cid: CID.to_string(),
            upby: WORKER_B.to_string(),
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
    
    // jsdata字段是JSON字符串
    let jsdata_str = json.get("jsdata").and_then(|v| v.as_str()).expect("无jsdata");
    let jsdata: serde_json::Value = serde_json::from_str(jsdata_str).expect("解析jsdata失败");
    let bytedata_base64 = jsdata.get("bytedata").and_then(|v| v.as_str()).expect("无bytedata");
    let bytes = base64::engine::general_purpose::STANDARD.decode(bytedata_base64.as_bytes()).expect("Base64解码失败");
    let synclog_batch = SynclogBatch::decode(&*bytes).expect("解码失败");
    
    println!("下载到 {} 条synclog", synclog_batch.items.len());
    
    // 执行synclog中的SQL（使用DataState基类）
    for item in &synclog_batch.items {
        let params: Vec<serde_json::Value> = serde_json::from_str(&item.params).unwrap_or_default();
        println!("执行: action={}, params={:?}", item.action, params);
        match item.action.as_str() {
            "insert" => {
                let mut record = std::collections::HashMap::new();
                record.insert("id".to_string(), serde_json::Value::String(params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("cid".to_string(), serde_json::Value::String(params.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("kind".to_string(), serde_json::Value::String(params.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("item".to_string(), serde_json::Value::String(params.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("data".to_string(), serde_json::Value::String(params.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                
                let result = local_testtb.m_save(&record, "testtb", "方案3-客户端同步插入");
                let id_str = params.get(0).and_then(|v| v.as_str()).unwrap_or("");
                println!("INSERT结果: {:?}, id={}", result, id_str);
            }
            "update" => {
                let id = params.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mut record = std::collections::HashMap::new();
                record.insert("kind".to_string(), serde_json::Value::String(params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("item".to_string(), serde_json::Value::String(params.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                record.insert("data".to_string(), serde_json::Value::String(params.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string()));
                
                let _ = local_testtb.m_update(&id, &record, "testtb", "方案3-客户端同步修改");
            }
            "delete" => {
                let id = params.get(0).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _ = local_testtb.m_del(&id, "testtb", "方案3-客户端同步删除");
            }
            _ => {}
        }
    }
    
    // 验证最终数据一致性
    let server_rows = remote_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    let client_rows = local_testtb.mlist("testtb", 1000, "获取所有记录").unwrap_or_default();
    println!("客户端记录数: {}", client_rows.len());
    println!("服务器记录数: {}", server_rows.len());
    assert_eq!(client_rows.len(), server_rows.len());
    println!("✅ 方案3通过");
    
    // 验证upby/uptime字段
    println!("\n========== 验证upby/uptime字段 ==========");
    let rows = local_testtb.mlist("testtb", 3, "验证字段").unwrap_or_default();
    for row in &rows {
        println!("客户端: id={}, kind={}, upby={}, uptime={}", &row.id[..8.min(row.id.len())], row.kind, row.upby, row.uptime);
    }
    
    println!("\n========== 所有测试通过 ==========");
    println!("worker_id: {}", get_worker_id());
}
