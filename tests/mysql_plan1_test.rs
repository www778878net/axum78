//! 双 Worker 同步测试
//!
//! Worker A 和 Worker B 各自操作本地 SQLite → 推送 datasync 到中心 →
//! getByWorker 拉取对方日志 → 本地重放 → 验证三份数据一致

use axum78::apisvc::backsvc::datasync_mysql::{DatasyncItem, DatasyncBatch};
use axum78::{create_router, LOVERS_CREATE_SQL, LOVERS_AUTH_CREATE_SQL};
use base64::{Engine as _, engine::general_purpose};
use prost::Message;
use datastate::{LocalDB, next_id_string};
use serde_json::Value;

const PORT: u16 = 18711;
const URL: &str = "http://127.0.0.1:18711";
const CID: &str = "318225842662547456";
const WA: &str = "worker-a";
const WB: &str = "worker-b";

const ISQL: &str = "INSERT INTO testtb5 (`id`,`cid`,`kind`,`item`,`data`,`d2`,`d3`,`d4`,`d5`,`d6`,`upby`,`uptime`,`remark`,`remark2`,`remark3`,`remark4`,`remark5`,`remark6`) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
const USQL: &str = "UPDATE testtb5 SET `kind`=?,`item`=?,`data`=?,`d2`=?,`upby`=?,`uptime`=?,`remark`=? WHERE `id`=?";
const DSQL: &str = "DELETE FROM testtb5 WHERE `id`=?";

const DDL: &str = "CREATE TABLE IF NOT EXISTS testtb5 (id INTEGER PRIMARY KEY,cid INTEGER DEFAULT 0,kind TEXT DEFAULT '',item TEXT DEFAULT '',data TEXT DEFAULT '',d2 TEXT DEFAULT '',d3 TEXT DEFAULT '',d4 TEXT DEFAULT '',d5 TEXT DEFAULT '',d6 TEXT DEFAULT '',upby TEXT DEFAULT '',uptime TEXT DEFAULT '',remark TEXT DEFAULT '',remark2 TEXT DEFAULT '',remark3 TEXT DEFAULT '',remark4 TEXT DEFAULT '',remark5 TEXT DEFAULT '',remark6 TEXT DEFAULT '')";

fn item(worker: &str, act: &str, id: &str, kind: &str, itm: &str, dat: &str) -> DatasyncItem {
    let (cmd, params): (&str, Vec<&str>) = match act {
        "insert" => (ISQL, vec![id, CID, kind, itm, dat, "", "", "", "", "", worker, "2026-07-10T17:30", "axum78test", "", "", "", "", ""]),
        "update" => (USQL, vec![kind, itm, dat, "", worker, "2026-07-10T17:30", "axum78test", id]),
        "delete" => (DSQL, vec![id]),
        _ => unreachable!(),
    };
    DatasyncItem{id:String::new(),apisys:"v1".into(),apimicro:"iflow".into(),apiobj:"datasync".into(),tbname:"testtb5".into(),action:act.into(),cmdtext:cmd.into(),params:serde_json::to_string(&params).unwrap(),idrow:id.into(),worker:worker.into(),synced:0,cmdtextmd5:String::new(),cid:CID.into(),upby:worker.into()}
}

// ====== HTTP ======

async fn push(client: &reqwest::Client, items: Vec<DatasyncItem>) -> (usize, usize) {
    let b = DatasyncBatch{items}; let js = general_purpose::STANDARD.encode(&b.encode_to_vec());
    let j: Value = client.post(format!("{}/apisvc/backsvc/datasync_mysql/maddmany",URL))
        .header("Content-Type","application/json")
        .body(serde_json::to_string(&serde_json::json!({"sid":"","jsdata":js})).unwrap())
        .send().await.expect("push").json().await.expect("json");
    let back = &j["back"];
    (back["success_ids"].as_array().map(|a|a.len()).unwrap_or(0),
     back["failed"].as_array().map(|a|a.len()).unwrap_or(0))
}

async fn pull(client: &reqwest::Client, my: &str, cur: &str) -> DatasyncBatch {
    let j: Value = client.post(format!("{}/apisvc/backsvc/datasync_mysql/getbyworker",URL))
        .header("Content-Type","application/json")
        .body(serde_json::to_string(&serde_json::json!({"sid":format!("cid|{}",my),"mid":cur,"getnumber":50})).unwrap())
        .send().await.expect("pull").json().await.expect("json");
    let b64 = j["back"]["bytedata"].as_str().unwrap();
    DatasyncBatch::decode(&*general_purpose::STANDARD.decode(b64).unwrap()).unwrap()
}

// ====== 本地 SQLite ======

fn extract_cols(cmd: &str) -> Vec<&str> {
    if let (Some(s),Some(e)) = (cmd.find('('), cmd.find("VALUES")) {
        cmd[s+1..e].split(',').map(|c|c.trim().trim_matches('`').trim_matches(')').trim()).filter(|c|!c.is_empty()).collect()
    } else { vec![] }
}

async fn replay(db: &LocalDB, items: &[DatasyncItem]) -> usize {
    let mut n = 0;
    for it in items {
        let p: Vec<Value> = serde_json::from_str(&it.params).unwrap_or_default();
        let sql = match it.action.as_str() {
            "insert" => {
                let cols = extract_cols(&it.cmdtext);
                let vals: Vec<_> = cols.iter().enumerate().map(|(i,_)| {
                    if i < p.len() && p[i].is_string() { format!("'{}'",p[i].as_str().unwrap().replace('\'',"''")) }
                    else { "''".into() }
                }).collect();
                format!("INSERT OR REPLACE INTO testtb5 ({}) VALUES ({})", cols.join(","), vals.join(","))
            }
            "update" => {
                let set_cols = ["kind","item","data","d2","upby","uptime","remark"];
                let sets: Vec<_> = set_cols.iter().enumerate()
                    .filter(|(i,_)| *i < p.len().saturating_sub(1))
                    .map(|(i,c)| format!("{}='{}'",c,p[i].as_str().unwrap_or("").replace('\'',"''"))).collect();
                let idv = p.last().and_then(|v|v.as_str()).unwrap_or("");
                format!("UPDATE testtb5 SET {} WHERE id='{}'", sets.join(","), idv.replace('\'',"''"))
            }
            "delete" => {
                let idv = p.first().and_then(|v|v.as_str()).unwrap_or("");
                format!("DELETE FROM testtb5 WHERE id='{}'", idv.replace('\'',"''"))
            }
            _ => continue,
        };
        match db.execute(&sql).await { Ok(_) => { n += 1; } Err(e) => { eprintln!("  replay ERR: {} SQL={}", e, &sql[..100.min(sql.len())]); } }
    }
    n
}

async fn ids(db: &LocalDB) -> Vec<String> {
    db.query("SELECT id FROM testtb5 ORDER BY id", &[]).await.unwrap_or_default()
        .iter().flat_map(|r|r.get("id").and_then(|v|v.as_i64()).map(|i|i.to_string())
            .or_else(||r.get("id").and_then(|v|v.as_str()).map(|s|s.to_string()))).collect()
}

// ====== 测试 ======

#[tokio::test]
async fn test_two_workers_sync() {
    std::env::set_var("MYSQL_DATABASE","saas2026");
    axum78::apisvc::backsvc::datasync_mysql::register_controller();

    let dd = LocalDB::default_instance().unwrap();
    dd.execute(LOVERS_CREATE_SQL).await.ok();
    dd.execute(LOVERS_AUTH_CREATE_SQL).await.ok();

    let a = LocalDB::with_path("docs/config/worker_a.db").unwrap();
    a.execute("DROP TABLE IF EXISTS testtb5").await.ok();
    a.execute(DDL).await.unwrap();
    let b = LocalDB::with_path("docs/config/worker_b.db").unwrap();
    b.execute("DROP TABLE IF EXISTS testtb5").await.ok();
    b.execute(DDL).await.unwrap();

    let app = create_router();
    let l = tokio::net::TcpListener::bind(format!("127.0.0.1:{}",PORT)).await.unwrap();
    let _s = tokio::spawn(async move { axum::serve(l,app).await.unwrap(); });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    let c = reqwest::Client::new();

    // === Worker A ===
    println!("\n===== Worker A =====");
    let a1 = next_id_string(); let a2 = next_id_string();
    println!("  IDs: {} {}", a1, a2);
    let ai = vec![
        item(WA,"insert",&a1,"ka1","ia1","da1"),
        item(WA,"insert",&a2,"ka2","ia2","da2"),
        item(WA,"update",&a1,"ka1_upd","ia1_upd","da1_upd"),
        item(WA,"delete",&a2,"","",""),
    ];
    replay(&a, &ai).await;
    println!("  A 本地: {} 行", ids(&a).await.len());
    let (ok,fail) = push(&c, ai).await;
    println!("  A 推送中心: ok={} fail={}", ok, fail);

    // === Worker B ===
    println!("\n===== Worker B =====");
    let b1 = next_id_string(); let b2 = next_id_string();
    println!("  IDs: {} {}", b1, b2);
    let bi = vec![
        item(WB,"insert",&b1,"kb1","ib1","db1"),
        item(WB,"insert",&b2,"kb2","ib2","db2"),
        item(WB,"update",&b1,"kb1_upd","ib1_upd","db1_upd"),
        item(WB,"delete",&b2,"","",""),
    ];
    replay(&b, &bi).await;
    println!("  B 本地: {} 行", ids(&b).await.len());
    let (ok,fail) = push(&c, bi).await;
    println!("  B 推送中心: ok={} fail={}", ok, fail);

    // === A 拉 B 的变更 ===
    println!("\n===== A 拉取 B 的变更 =====");
    let ap = pull(&c, WA, "0").await;
    println!("  拉回 {} 条", ap.items.len());
    replay(&a, &ap.items).await;
    let a_ids = ids(&a).await;
    println!("  A 最终: {} 行 {:?}", a_ids.len(), a_ids);

    // === B 拉 A 的变更 ===
    println!("\n===== B 拉取 A 的变更 =====");
    let bp = pull(&c, WB, "0").await;
    println!("  拉回 {} 条", bp.items.len());
    replay(&b, &bp.items).await;
    let b_ids = ids(&b).await;
    println!("  B 最终: {} 行 {:?}", b_ids.len(), b_ids);

    // === 验证 ===
    println!("\n===== 验证 A == B =====");
    assert_eq!(a_ids.len(), b_ids.len());
    assert_eq!(a_ids.len(), 2, "双方都应有 2 行");
    for i in 0..a_ids.len() { assert_eq!(a_ids[i], b_ids[i]); }

    a.execute("DROP TABLE IF EXISTS testtb5").await.ok();
    b.execute("DROP TABLE IF EXISTS testtb5").await.ok();
    println!("\n✅ 双 Worker 交叉同步测试通过");
}
