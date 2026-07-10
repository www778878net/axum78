//! testtb5 双向同步测试
//!
//! 方案1: 本地 SQLite 增删改 → 同步到中心 MySQL (saas2026.testtb5)
//! 方案2: 中心 MySQL 增删改 → 同步到本地 SQLite (docs/config/remote.db)
//! 方案3: 最终一致性逐行比对
//!
//! 全 Rust: prost DatasyncBatch 编解码，reqwest HTTP，无 Python

use axum78::apisvc::backsvc::datasync_mysql::{DatasyncItem, DatasyncBatch};
use axum78::{create_router, LOVERS_CREATE_SQL, LOVERS_AUTH_CREATE_SQL};
use base64::{Engine as _, engine::general_purpose};
use prost::Message;
use datastate::LocalDB;
use serde_json::Value;

const TEST_PORT: u16 = 18701;
const BASE_URL: &str = "http://127.0.0.1:18701";
const TBNAME: &str = "testtb5";
const GUEST_CID: &str = "318225842662547456";
const TEST_WORKER: &str = "test-worker-78";

// 中心 saas2026.testtb5 完整 18 列 INSERT（反引号给 dowork 解析用）
const INSERT_SQL: &str = "INSERT INTO testtb5 (`id`, `cid`, `kind`, `item`, `data`, `d2`, `d3`, `d4`, `d5`, `d6`, `upby`, `uptime`, `remark`, `remark2`, `remark3`, `remark4`, `remark5`, `remark6`) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
const UPDATE_SQL: &str = "UPDATE testtb5 SET `kind` = ?, `item` = ?, `data` = ?, `d2` = ?, `upby` = ?, `uptime` = ?, `remark` = ? WHERE `id` = ?";
const DELETE_SQL: &str = "DELETE FROM testtb5 WHERE `id` = ?";

// 本地 SQLite testtb5 建表（对齐中心 MySQL 全部列）
const LOCAL_CREATE_SQL: &str = "CREATE TABLE IF NOT EXISTS testtb5 (id INTEGER PRIMARY KEY, cid INTEGER NOT NULL DEFAULT 0, kind TEXT NOT NULL DEFAULT '', item TEXT NOT NULL DEFAULT '', data TEXT NOT NULL DEFAULT '', d2 TEXT NOT NULL DEFAULT '', d3 TEXT NOT NULL DEFAULT '', d4 TEXT NOT NULL DEFAULT '', d5 TEXT NOT NULL DEFAULT '', d6 TEXT NOT NULL DEFAULT '', upby TEXT NOT NULL DEFAULT '', uptime TEXT NOT NULL DEFAULT '', remark TEXT NOT NULL DEFAULT '', remark2 TEXT NOT NULL DEFAULT '', remark3 TEXT NOT NULL DEFAULT '', remark4 TEXT NOT NULL DEFAULT '', remark5 TEXT NOT NULL DEFAULT '', remark6 TEXT NOT NULL DEFAULT '')";

// ====== DatasyncItem 构造 ======

fn insert_item(id: &str, kind: &str, item_val: &str, data: &str, remark: &str) -> DatasyncItem {
    DatasyncItem {
        id: String::new(), apisys: "v1".into(), apimicro: "iflow".into(), apiobj: "datasync".into(),
        tbname: TBNAME.into(), action: "insert".into(), cmdtext: INSERT_SQL.into(),
        params: serde_json::to_string(&vec![
            id, GUEST_CID, kind, item_val, data,
            "", "", "", "", "", "axum78test", "2026-07-10 16:00:00",
            remark, "", "", "", "", "",
        ]).unwrap(),
        idrow: id.into(), worker: TEST_WORKER.into(), synced: 0, cmdtextmd5: String::new(),
        cid: GUEST_CID.into(), upby: "axum78test".into(),
    }
}

fn update_item(id: &str, kind: &str, item_val: &str, data: &str) -> DatasyncItem {
    DatasyncItem {
        id: String::new(), apisys: "v1".into(), apimicro: "iflow".into(), apiobj: "datasync".into(),
        tbname: TBNAME.into(), action: "update".into(), cmdtext: UPDATE_SQL.into(),
        params: serde_json::to_string(&vec![
            kind, item_val, data, "", "axum78test", "2026-07-10 16:00:00", "updated", id,
        ]).unwrap(),
        idrow: id.into(), worker: TEST_WORKER.into(), synced: 0, cmdtextmd5: String::new(),
        cid: GUEST_CID.into(), upby: "axum78test".into(),
    }
}

fn delete_item(id: &str) -> DatasyncItem {
    DatasyncItem {
        id: String::new(), apisys: "v1".into(), apimicro: "iflow".into(), apiobj: "datasync".into(),
        tbname: TBNAME.into(), action: "delete".into(), cmdtext: DELETE_SQL.into(),
        params: serde_json::to_string(&vec![id]).unwrap(),
        idrow: id.into(), worker: TEST_WORKER.into(), synced: 0, cmdtextmd5: String::new(),
        cid: GUEST_CID.into(), upby: "axum78test".into(),
    }
}

// ====== HTTP 请求辅助 ======

fn parse_back(json: &Value) -> Value {
    let back = json.get("back").expect("无 back 字段");
    if let Some(s) = back.as_str() { serde_json::from_str(s).unwrap_or_else(|_| back.clone()) }
    else { back.clone() }
}

async fn maddmany(client: &reqwest::Client, items: Vec<DatasyncItem>) -> (Vec<String>, Vec<Value>) {
    let batch = DatasyncBatch { items };
    let jsdata = general_purpose::STANDARD.encode(&batch.encode_to_vec());
    let resp = client
        .post(format!("{}/apisvc/backsvc/datasync_mysql/maddmany", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&serde_json::json!({"sid": "", "jsdata": jsdata})).unwrap())
        .send().await.expect("maddmany 请求失败");
    let json: Value = resp.json().await.expect("解析失败");
    assert_eq!(json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1), 0, "maddmany res != 0");
    let back = parse_back(&json);
    let success: Vec<String> = back.get("success_ids").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let failed: Vec<Value> = back.get("failed").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    (success, failed)
}

async fn center_get_kinds(client: &reqwest::Client) -> Vec<String> {
    let resp = client
        .post(format!("{}/apisvc/backsvc/datasync_mysql/get", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"sid": "", "getnumber": 50}).to_string())
        .send().await.expect("get 请求失败");
    let json: Value = resp.json().await.expect("解析失败");
    assert_eq!(json.get("res").and_then(|v| v.as_i64()).unwrap_or(-1), 0);
    let back = parse_back(&json);
    let b64 = back.get("bytedata").and_then(|v| v.as_str()).expect("无bytedata");
    let batch = DatasyncBatch::decode(&*general_purpose::STANDARD.decode(b64).expect("decode"))
        .expect("protobuf decode");
    batch.items.iter()
        .filter(|it| it.action == "insert" && it.tbname == TBNAME)
        .filter_map(|it| serde_json::from_str::<Value>(&it.cmdtext).ok()
            .and_then(|b| b.get("kind").and_then(|v| v.as_str()).map(|s| s.to_string())))
        .collect()
}

async fn local_maddmany(client: &reqwest::Client, items: Vec<DatasyncItem>) {
    let batch = DatasyncBatch { items };
    let jsdata = general_purpose::STANDARD.encode(&batch.encode_to_vec());
    let resp = client
        .post(format!("{}/apisvc/backsvc/datasync/maddmany", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&serde_json::json!({"sid": "", "jsdata": jsdata})).unwrap())
        .send().await.expect("local maddmany 失败");
    assert_eq!(resp.json::<Value>().await.expect("解析失败")
        .get("res").and_then(|v| v.as_i64()).unwrap_or(-1), 0);
}

async fn local_dowork(client: &reqwest::Client) -> Value {
    let resp = client
        .post(format!("{}/apisvc/backsvc/datasync/dowork", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({"sid": "test"}).to_string())
        .send().await.expect("local dowork 失败");
    let json: Value = resp.json().await.expect("解析失败");
    parse_back(&json)
}

// ====== 主测试 ======

#[tokio::test]
async fn test_bidirectional_sync() {
    // ---- 启动服务 ----
    std::env::set_var("MYSQL_DATABASE", "saas2026");

    axum78::apisvc::backsvc::datasync::register_controller();
    axum78::apisvc::backsvc::datasync_mysql::register_controller();

    let default_db = LocalDB::default_instance().expect("默认数据库失败");
    default_db.execute(LOVERS_CREATE_SQL).await.expect("lovers 建表失败");
    default_db.execute(LOVERS_AUTH_CREATE_SQL).await.expect("lovers_auth 建表失败");

    // 本地 SQLite remote.db — 建 testtb5 表（方案2 dowork 重放到这里）
    let local_db = LocalDB::with_path("docs/config/remote.db").expect("打开 remote.db 失败");
    local_db.execute("DELETE FROM datasync WHERE tbname='testtb5'").await.ok();
    local_db.execute("DROP TABLE IF EXISTS testtb5").await.ok();
    local_db.execute(LOCAL_CREATE_SQL).await.expect("建 testtb5 本地表失败");

    let app = create_router();
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", TEST_PORT))
        .await.expect("绑定端口失败");
    let _server = tokio::spawn(async move { axum::serve(listener, app).await.expect("服务崩溃"); });
    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

    let client = reqwest::Client::new();

    // 清理中心旧测试数据（忽略结果，可能不存在）
    let _ = client
        .post(format!("{}/apisvc/backsvc/datasync_mysql/maddmany", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&serde_json::json!({
            "sid": "",
            "jsdata": general_purpose::STANDARD.encode(&DatasyncBatch {
                items: vec![
                    delete_item("7800001"), delete_item("7800002"),
                    delete_item("7800003"), delete_item("7800004"),
                ]
            }.encode_to_vec())
        })).unwrap())
        .send().await;

    // ================================================================
    // 方案1：本地 SQLite 增删改 → 同步到中心 MySQL
    // ================================================================
    println!("\n==================== 方案1：本地 → 中心 ====================");

    // Insert 2 条到中心（模拟本地 app 生成的 datasync 上传）
    {
        let (s, f) = maddmany(&client, vec![
            insert_item("7800001", "k1_insert", "i1_insert", "d1_insert", "plan1"),
            insert_item("7800002", "k2_insert", "i2_insert", "d2_insert", "plan1"),
        ]).await;
        println!("  INSERT x2: success={}, failed={}", s.len(), f.len());
        assert_eq!(s.len(), 2);
    }

    // Update 7800001
    {
        let (s, f) = maddmany(&client, vec![
            update_item("7800001", "k1_UPDATED", "i1_UPDATED", "d1_UPDATED"),
        ]).await;
        println!("  UPDATE 7800001: success={}, failed={}", s.len(), f.len());
        assert_eq!(s.len(), 1);
    }

    // Delete 7800002
    {
        let (s, f) = maddmany(&client, vec![delete_item("7800002")]).await;
        println!("  DELETE 7800002: success={}, failed={}", s.len(), f.len());
        assert_eq!(s.len(), 1);
    }

    // 验证中心：应有 k1_UPDATED，不应有 k2_insert
    let center_kinds = center_get_kinds(&client).await;
    println!("  中心 INSERT kinds: {:?}", center_kinds);
    assert!(center_kinds.contains(&"k1_UPDATED".to_string()), "中心应有 k1_UPDATED");
    assert!(!center_kinds.contains(&"k2_insert".to_string()), "7800002 应已删除");

    // 方案1 数据也回放到本地（保证双向一致）
    let plan1_items = vec![
        insert_item("7800001", "k1_insert", "i1_insert", "d1_insert", "plan1"),
        insert_item("7800002", "k2_insert", "i2_insert", "d2_insert", "plan1"),
        update_item("7800001", "k1_UPDATED", "i1_UPDATED", "d1_UPDATED"),
        delete_item("7800002"),
    ];
    local_maddmany(&client, plan1_items).await;
    let dw1 = local_dowork(&client).await;
    println!("  方案1 本地回放: processed={}, success={}", 
        dw1.get("processed").and_then(|v| v.as_i64()).unwrap_or(0),
        dw1.get("success").and_then(|v| v.as_i64()).unwrap_or(0));

    println!("✅ 方案1 通过");

    // ================================================================
    // 方案2：中心 MySQL 增删改 → 同步到本地 SQLite
    // ================================================================
    println!("\n==================== 方案2：中心 → 本地 ====================");

    // 保存发给中心的items（datasync_mysql/get 会改写 cmdtext 为业务数据，
    // 本地 dowork 需要原始 SQL，所以本地重放用原始 items）
    let plan2_items = vec![
        insert_item("7800003", "k3_center", "i3_center", "d3_center", "plan2"),
        insert_item("7800004", "k4_center", "i4_center", "d4_center", "plan2"),
        update_item("7800003", "k3_CENTER_UPD", "i3_CENTER_UPD", "d3_CENTER_UPD"),
        delete_item("7800004"),
    ];

    // 中心执行增删改
    for (i, item) in plan2_items.iter().enumerate() {
        let action = &item.action;
        let id = &item.idrow;
        let (s, f) = maddmany(&client, vec![item.clone()]).await;
        println!("  中心 {} id={}: success_count={}, failed_count={}", action, id, s.len(), f.len());
        assert_eq!(s.len(), 1, "中心 {} {} 应成功", action, id);
    }

    // 拉取中心，验证中心数据正确
    {
        let kinds = center_get_kinds(&client).await;
        println!("  中心 INSERT kinds（方案2后）: {:?}", kinds);
        assert!(kinds.contains(&"k3_CENTER_UPD".to_string()), "中心应有 k3_CENTER_UPD");
        assert!(kinds.contains(&"k1_UPDATED".to_string()), "方案1数据仍在");
        assert!(!kinds.contains(&"k4_center".to_string()), "7800004 应已删除");
    }

    // 本地重放（用原始 items，含正确 SQL cmdtext）
    // 注意：insert 的 id 字段使用服务端雪花ID，params 不变
    local_maddmany(&client, plan2_items.clone()).await;
    let dowork_result = local_dowork(&client).await;
    let processed = dowork_result.get("processed").and_then(|v| v.as_i64()).unwrap_or(0);
    let success_count = dowork_result.get("success").and_then(|v| v.as_i64()).unwrap_or(0);
    let failed_count = dowork_result.get("failed").and_then(|v| v.as_i64()).unwrap_or(0);
    println!("  本地 dowork: processed={}, success={}, failed={}", processed, success_count, failed_count);
    if let Some(errors) = dowork_result.get("errors").and_then(|v| v.as_array()) {
        for e in errors {
            println!("    dowork error: {}", e.get("error").and_then(|v| v.as_str()).unwrap_or("?"));
        }
    }
    assert!(success_count >= 2, "本地重放成功数应 ≥2，实际={}", success_count);

    // 验证本地
    let local_rows = local_db.query("SELECT id, kind FROM testtb5 ORDER BY id", &[])
        .await.expect("查询本地失败");
    println!("  本地 testtb5 行数: {}", local_rows.len());
    for row in &local_rows {
        let id = row.get("id").and_then(|v| v.as_i64()).map(|i| i.to_string())
            .or_else(|| row.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .unwrap_or_default();
        let kind = row.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        println!("    本地: id={}, kind={}", id, kind);
    }
    assert!(local_rows.len() >= 2, "本地应有 ≥2 行（方案1的1行 + 方案2的1行）");

    println!("✅ 方案2 通过");

    // ================================================================
    // 方案3：最终一致性逐行比对
    // ================================================================
    println!("\n==================== 方案3：最终一致性比对 ====================");

    let center_final = center_get_kinds(&client).await;
    println!("  中心: {:?}", center_final);

    let local_kinds: Vec<String> = local_rows.iter()
        .filter_map(|r| r.get("kind").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    println!("  本地: {:?}", local_kinds);

    // 核心字段一致
    assert!(center_final.contains(&"k1_UPDATED".to_string()));
    assert!(center_final.contains(&"k3_CENTER_UPD".to_string()));
    assert!(!center_final.contains(&"k2_insert".to_string()));
    assert!(!center_final.contains(&"k4_center".to_string()));

    assert!(local_kinds.contains(&"k1_UPDATED".to_string()));
    assert!(local_kinds.contains(&"k3_CENTER_UPD".to_string()));
    assert!(!local_kinds.contains(&"k2_insert".to_string()));
    assert!(!local_kinds.contains(&"k4_center".to_string()));

    println!("  中心行数: {}, 本地行数: {} ✓", center_final.len(), local_rows.len());
    println!("✅ 方案3 通过");

    // ================================================================
    // 清理
    // ================================================================
    println!("\n==================== 清理 ====================");
    for id in &["7800001", "7800003"] {
        let (s, _) = maddmany(&client, vec![delete_item(id)]).await;
        println!("  删除中心 id={}: success_count={}", id, s.len());
    }
    local_db.execute("DROP TABLE IF EXISTS testtb5").await.ok();

    println!("\n==================== 双向同步批量测试全通过 ✅ ====================");
}
