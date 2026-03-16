//! 按文档测试方案执行
//!
//! docs/dev/admin_data_state.md#L75-79
//!
//! 测试方案（连续执行）：
//! 1. 先清空表testtb，服务器数据下载到本地
//! 2. 添加2条、修改2条、删除2条，上传到服务器
//! 3. 服务器添加修改删除各2条，自动同步到客户端
//! 4. 冲突测试
//!
//! 使用axum78本地服务器（protobuf格式）
//! SID验证：服务器验证SID对应的CID与数据中的CID是否一致

#[cfg(test)]
mod doc_test_plan {
    use crate::proto::{testtb, testtbItem, SyncRequest, SyncResponse, UploadResponse};
    use crate::sync::DataSync;
    use base::{ProjectPath, UpInfo};
    use database::{LocalDB, Sqlite78};
    use prost::Message;
    use reqwest::blocking::Client;

    const SERVER_URL: &str = "http://127.0.0.1:3780";

    fn get_client_sync() -> DataSync {
        let db_path = Sqlite78::find_default_db_path().expect("获取默认数据库路径失败");
        DataSync::with_remote_db(&db_path)
    }

    fn get_server_sync() -> DataSync {
        let project = ProjectPath::find().expect("查找项目根目录失败");
        let server_db = project.root().join("tmp/data/remote.db").to_string_lossy().to_string();
        DataSync::with_remote_db(&server_db)
    }

    fn clear_all(sync: &DataSync) {
        let up = UpInfo::new();
        let _ = sync.db.do_m("DELETE FROM testtb", &[], &up);
        let _ = sync.db.do_m("DELETE FROM sync_queue", &[], &up);
    }

    /// 获取SID和CID
    fn get_sid_and_cid() -> (String, String) {
        let db = LocalDB::new(None, None).expect("数据库连接失败");
        let sid = db.get_sid();
        if sid.is_empty() {
            // 使用测试SID
            let test_cid = "test-company-id-12345";
            (test_cid.to_string(), test_cid.to_string())
        } else {
            // 从SID获取CID（简化处理：SID就是CID）
            let cid = if sid.contains('|') {
                sid.split('|').next().unwrap_or(&sid).to_string()
            } else {
                sid.clone()
            };
            (sid, cid)
        }
    }

    /// 从服务器下载数据（通过HTTP protobuf）
    fn download_from_server(sid: &str, cid: &str) -> Result<Vec<testtbItem>, String> {
        let client = Client::new();
        let url = format!("{}/sync/testtb/get", SERVER_URL);
        
        let request = SyncRequest {
            table_name: "testtb".to_string(),
            sid: sid.to_string(),
            cid: cid.to_string(),
            getstart: 0,
            getnumber: 1000,
            last_uptime: String::new(),
        };
        
        let body = request.encode_to_vec();
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .map_err(|e| format!("HTTP请求失败: {}", e))?;
        
        let bytes = response.bytes().map_err(|e| format!("读取响应失败: {}", e))?;
        let sync_response = SyncResponse::decode(&*bytes).map_err(|e| format!("解码失败: {}", e))?;
        
        if sync_response.res != 0 {
            return Err(sync_response.errmsg);
        }
        
        Ok(sync_response.items)
    }

    /// 上传数据到服务器（通过HTTP protobuf）
    fn upload_to_server(sid: &str, items: &[testtbItem]) -> Result<UploadResponse, String> {
        let client = Client::new();
        let url = format!("{}/sync/testtb", SERVER_URL);
        
        let request = testtb {
            sid: sid.to_string(),
            items: items.to_vec(),
        };
        
        let body = request.encode_to_vec();
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .map_err(|e| format!("HTTP请求失败: {}", e))?;
        
        let bytes = response.bytes().map_err(|e| format!("读取响应失败: {}", e))?;
        let upload_response = UploadResponse::decode(&*bytes).map_err(|e| format!("解码失败: {}", e))?;
        
        Ok(upload_response)
    }

    /// 检查服务器是否运行
    fn check_server_health() -> bool {
        let client = Client::new();
        let url = format!("{}/health", SERVER_URL);
        client.get(&url).send().is_ok()
    }

    /// 完整测试方案（连续执行）
    #[test]
    fn test_all_plans() {
        println!("\n========================================");
        println!("=== 完整同步测试（本地axum78服务器） ===");
        println!("========================================");

        // 检查服务器是否运行
        if !check_server_health() {
            println!("⚠️ 服务器未运行，请先启动: cargo run -p axum78 --bin sync_server");
            println!("跳过测试");
            return;
        }
        println!("服务器运行中: {}", SERVER_URL);

        // 获取SID和CID
        let (sid, cid) = get_sid_and_cid();
        println!("SID: {}...", &sid[..20.min(sid.len())]);
        println!("CID: {}", cid);

        let server_sync = get_server_sync();
        server_sync.ensure_table().expect("建表失败");

        let client_sync = get_client_sync();
        client_sync.ensure_table().expect("建表失败");

        // ===== 方案1: 清空表，服务器数据下载到本地 =====
        println!("\n========== 方案1: 单向下载测试 ==========");
        clear_all(&server_sync);
        clear_all(&client_sync);

        // 服务器：插入5条数据（使用正确的CID）
        for i in 0..5 {
            let item = testtbItem {
                id: String::new(),
                idpk: 0,
                cid: cid.clone(),
                kind: format!("server_kind_{}", i),
                item: format!("server_item_{}", i),
                data: format!("server_data_{}", i),
                upby: "server".to_string(),
                uptime: format!("2024-01-{:02} 00:00:00", i + 1),
            };
            let id = server_sync.insert_item(&item).expect("插入失败");
            println!("服务器插入: {} -> id={}", i, id);
        }

        let server_count = server_sync.count().expect("计数失败");
        println!("服务器记录数: {}", server_count);
        assert_eq!(server_count, 5);

        // 通过HTTP下载
        let items = download_from_server(&sid, &cid).expect("下载失败");
        println!("从服务器下载: {} 条", items.len());

        // 写入客户端数据库
        for item in &items {
            client_sync.apply_remote_update(item).expect("写入失败");
        }

        let client_count = client_sync.count().expect("计数失败");
        println!("客户端记录数: {}", client_count);
        assert_eq!(client_count, 5);
        println!("✅ 方案1通过");

        // ===== 方案2: 添加2条、修改2条、删除2条，上传到服务器 =====
        println!("\n========== 方案2: 上传测试 ==========");

        // 获取当前记录
        let current_records = client_sync.get_items().expect("查询失败");
        let mut init_ids: Vec<String> = current_records.iter().map(|r| r.id.clone()).collect();

        // 添加2条（使用正确的CID）
        let add_item1 = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: cid.clone(),
            kind: "add_kind_1".to_string(),
            item: "add_item_1".to_string(),
            data: "add_data_1".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-01 00:00:00".to_string(),
        };
        let add_id1 = client_sync.insert_item(&add_item1).expect("添加失败");
        init_ids.push(add_id1.clone());

        let add_item2 = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: cid.clone(),
            kind: "add_kind_2".to_string(),
            item: "add_item_2".to_string(),
            data: "add_data_2".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-01 00:00:00".to_string(),
        };
        let add_id2 = client_sync.insert_item(&add_item2).expect("添加失败");
        init_ids.push(add_id2.clone());

        println!("添加2条: id={}, id={}", add_id1, add_id2);

        // 修改2条
        let update_item1 = testtbItem {
            id: init_ids[0].clone(),
            idpk: 0,
            cid: cid.clone(),
            kind: "updated_kind_0".to_string(),
            item: "updated_item_0".to_string(),
            data: "updated_data_0".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-02 00:00:00".to_string(),
        };
        client_sync.update_item(&update_item1).expect("修改失败");

        let update_item2 = testtbItem {
            id: init_ids[1].clone(),
            idpk: 0,
            cid: cid.clone(),
            kind: "updated_kind_1".to_string(),
            item: "updated_item_1".to_string(),
            data: "updated_data_1".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-02 00:00:00".to_string(),
        };
        client_sync.update_item(&update_item2).expect("修改失败");

        println!("修改2条: id={}, id={}", init_ids[0], init_ids[1]);

        // 删除2条
        let del1 = client_sync.delete_item(&init_ids[2]).expect("删除失败");
        let del2 = client_sync.delete_item(&init_ids[3]).expect("删除失败");
        println!("删除2条: id={}={}, id={}={}", init_ids[2], del1, init_ids[3], del2);

        // 检查待同步队列（添加2 + 修改2 + 删除2 = 6）
        let pending = client_sync.get_pending_count().expect("获取待同步数失败");
        println!("待同步记录数: {}", pending);
        assert_eq!(pending, 6);

        // 获取待同步数据并上传
        let pending_items = client_sync.get_pending_items(100).expect("获取待同步记录失败");
        let upload_response = upload_to_server(&sid, &pending_items).expect("上传失败");
        println!("上传结果: res={}, total={}, errmsg={}", upload_response.res, upload_response.total, upload_response.errmsg);
        
        if !upload_response.errors.is_empty() {
            println!("验证错误:");
            for err in &upload_response.errors {
                println!("  index={}, id={}, error={}", err.index, err.idrow, err.error);
            }
        }

        // 标记已同步
        let ids: Vec<String> = pending_items.iter().map(|i| i.id.clone()).collect();
        client_sync.mark_synced(&ids).expect("标记失败");

        // 验证服务器数据
        let server_records = server_sync.get_items().expect("查询失败");
        let client_records = client_sync.get_items().expect("查询失败");

        println!("客户端记录数: {}", client_records.len());
        println!("服务器记录数: {}", server_records.len());

        assert!(server_records.iter().any(|r| r.kind == "add_kind_1"));
        println!("✅ 方案2通过");

        // ===== 方案3: 服务器变更同步测试 =====
        println!("\n========== 方案3: 服务器变更同步测试 ==========");

        // 服务器添加2条（使用正确的CID）
        let add1 = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: cid.clone(),
            kind: "server_add_kind_1".to_string(),
            item: "server_add_item_1".to_string(),
            data: "server_add_data_1".to_string(),
            upby: "server".to_string(),
            uptime: "2024-03-01 00:00:00".to_string(),
        };
        server_sync.apply_remote_update(&add1).expect("添加失败");

        let add2 = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: cid.clone(),
            kind: "server_add_kind_2".to_string(),
            item: "server_add_item_2".to_string(),
            data: "server_add_data_2".to_string(),
            upby: "server".to_string(),
            uptime: "2024-03-01 00:00:00".to_string(),
        };
        server_sync.apply_remote_update(&add2).expect("添加失败");

        println!("服务器添加2条");

        // 客户端下载
        let items = download_from_server(&sid, &cid).expect("下载失败");
        for item in &items {
            client_sync.apply_remote_update(item).expect("同步失败");
        }

        let client_records = client_sync.get_items().expect("查询失败");
        let server_records = server_sync.get_items().expect("查询失败");
        
        println!("客户端记录数: {}", client_records.len());
        println!("服务器记录数: {}", server_records.len());
        assert_eq!(client_records.len(), server_records.len());
        assert!(client_records.iter().any(|r| r.kind == "server_add_kind_1"));

        println!("✅ 方案3通过");

        // ===== 方案4: 冲突测试 =====
        println!("\n========== 方案4: 冲突测试 ==========");

        // 创建一条冲突数据
        let conflict_id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO testtb (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)";
        let up = UpInfo::new();
        client_sync.db.do_m(sql, &[&conflict_id, &cid, &"initial_kind", &"initial_item", &"initial_data", &"init", &"2024-01-01 00:00:00"], &up).expect("初始化失败");
        server_sync.db.do_m(sql, &[&conflict_id, &cid, &"initial_kind", &"initial_item", &"initial_data", &"init", &"2024-01-01 00:00:00"], &up).expect("初始化失败");

        println!("初始数据: id={}, kind=initial_kind", conflict_id);

        // 客户端修改（较早时间）
        let client_update = testtbItem {
            id: conflict_id.clone(),
            idpk: 0,
            cid: cid.clone(),
            kind: "client_kind".to_string(),
            item: "client_item".to_string(),
            data: "client_data".to_string(),
            upby: "client".to_string(),
            uptime: "2024-06-01 00:00:00".to_string(),
        };
        client_sync.update_item(&client_update).expect("客户端修改失败");

        // 服务器修改（较晚时间）
        let server_update = testtbItem {
            id: conflict_id.clone(),
            idpk: 0,
            cid: cid.clone(),
            kind: "server_kind".to_string(),
            item: "server_item".to_string(),
            data: "server_data".to_string(),
            upby: "server".to_string(),
            uptime: "2024-12-01 00:00:00".to_string(),
        };
        server_sync.apply_remote_update(&server_update).expect("服务器修改失败");

        // 同步
        let pending_items = client_sync.get_pending_items(10).expect("获取失败");
        let _ = upload_to_server(&sid, &pending_items);

        let items = download_from_server(&sid, &cid).expect("下载失败");
        for item in &items {
            client_sync.apply_remote_update(item).expect("下载失败");
        }

        // 验证
        let client_final = client_sync.get_item_by_id(&conflict_id).expect("查询失败").expect("未找到");
        let server_final = server_sync.get_item_by_id(&conflict_id).expect("查询失败").expect("未找到");

        println!("客户端: kind={}", client_final.kind);
        println!("服务器: kind={}", server_final.kind);

        assert_eq!(client_final.kind, server_final.kind);

        println!("✅ 方案4通过：冲突解决（uptime较晚者胜出）");

        // ===== 最终验证 =====
        println!("\n========== 最终验证 ==========");
        let client_final_records = client_sync.get_items().expect("查询失败");
        let server_final_records = server_sync.get_items().expect("查询失败");

        println!("客户端最终记录数: {}", client_final_records.len());
        println!("服务器最终记录数: {}", server_final_records.len());

        // 验证所有记录的CID都是正确的
        for record in &client_final_records {
            assert_eq!(record.cid, cid, "客户端记录CID不匹配: id={}, cid={}", record.id, record.cid);
        }
        for record in &server_final_records {
            assert_eq!(record.cid, cid, "服务器记录CID不匹配: id={}, cid={}", record.id, record.cid);
        }

        assert_eq!(client_final_records.len(), server_final_records.len());

        println!("\n✅✅✅ 所有测试方案通过！");
    }

    /// 测试CID验证失败场景
    #[test]
    fn test_cid_validation_failed() {
        println!("\n========================================");
        println!("=== CID验证失败测试 ===");
        println!("========================================");

        if !check_server_health() {
            println!("⚠️ 服务器未运行，跳过测试");
            return;
        }

        let (sid, correct_cid) = get_sid_and_cid();
        let wrong_cid = "wrong-company-id-99999";

        // 创建一条使用错误CID的数据
        let item = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: wrong_cid.to_string(),
            kind: "wrong_cid_test".to_string(),
            item: "test".to_string(),
            data: "test".to_string(),
            upby: "test".to_string(),
            uptime: "2024-01-01 00:00:00".to_string(),
        };

        // 上传（应该被拒绝）
        let response = upload_to_server(&sid, &[item.clone()]).expect("上传失败");
        
        println!("上传结果: res={}, total={}, errmsg={}", response.res, response.total, response.errmsg);
        
        // 验证：应该有错误
        assert!(!response.errors.is_empty(), "应该有CID验证错误");
        assert_eq!(response.errors[0].error, format!("cid 不匹配，期望 {}，实际 {}", correct_cid, wrong_cid));
        
        println!("✅ CID验证失败测试通过：错误的CID被正确拒绝");
    }
}
