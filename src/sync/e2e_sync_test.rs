//! 端到端同步测试
//!
//! 验证两个客户端通过服务器同步后数据一致

#[cfg(test)]
mod e2e_sync_tests {
    use crate::proto::{testtb, testtbItem, SyncRequest, SyncResponse};
    use crate::sync::DataSync;
    use prost::Message;
    use reqwest::Client;

    const SERVER_URL: &str = "http://127.0.0.1:3780";

    #[tokio::test]
    async fn test_full_sync_e2e() {
        let client = Client::new();

        // 清理：删除旧数据库
        let _ = std::fs::remove_file("tmp/data/client_a.db");
        let _ = std::fs::remove_file("tmp/data/client_b.db");

        // 客户端A：插入100条数据
        let sync_a = DataSync::with_remote_db("tmp/data/client_a.db");
        sync_a.ensure_table().expect("建表失败");

        let mut inserted_ids = Vec::new();
        for i in 0..100 {
            let item = testtbItem {
                id: String::new(),
                idpk: 0,
                cid: "default".to_string(),
                kind: format!("kind_{}", i),
                item: format!("item_{}", i),
                data: format!("data_{}", i),
                upby: "client_a".to_string(),
                uptime: String::new(),
            };
            let id = sync_a.insert_item(&item).expect("插入失败");
            inserted_ids.push(id);
        }

        let count_a = sync_a.count().expect("计数失败");
        println!("客户端A插入 {} 条，总数: {}", inserted_ids.len(), count_a);

        let pending_a = sync_a.get_pending_count().expect("获取待同步数失败");
        println!("客户端A待同步: {} 条", pending_a);
        assert_eq!(pending_a, 100, "应该有100条待同步");

        // 客户端A：上传到服务器
        let items_a = sync_a.get_pending_items(200).expect("获取待同步记录失败");
        println!("获取待同步记录: {} 条", items_a.len());

        let msg = testtb { items: items_a.clone() };
        let encoded = msg.encode_to_vec();

        let response = client
            .post(&format!("{}/sync/testtb", SERVER_URL))
            .header("Content-Type", "application/octet-stream")
            .body(encoded)
            .send()
            .await
            .expect("请求失败");

        assert!(response.status().is_success());
        let bytes = response.bytes().await.expect("读取响应失败");
        let sync_response = SyncResponse::decode(&*bytes).expect("解码失败");
        println!("上传到服务器: {} 条", sync_response.total);

        // 标记已同步
        let ids: Vec<String> = items_a.iter().map(|i| i.id.clone()).collect();
        sync_a.mark_synced(&ids).expect("标记同步失败");

        let pending_a_after = sync_a.get_pending_count().expect("获取待同步数失败");
        println!("上传后客户端A待同步: {} 条", pending_a_after);
        assert_eq!(pending_a_after, 0, "上传后应该没有待同步记录");

        // 客户端B：从服务器下载
        let sync_b = DataSync::with_remote_db("tmp/data/client_b.db");
        sync_b.ensure_table().expect("建表失败");

        let request = SyncRequest {
            table_name: "testtb".to_string(),
            cid: "default".to_string(),
            getstart: 0,
            getnumber: 200,
            last_uptime: String::new(),
        };

        let response = client
            .post(&format!("{}/sync/testtb/get", SERVER_URL))
            .header("Content-Type", "application/octet-stream")
            .body(request.encode_to_vec())
            .send()
            .await
            .expect("请求失败");

        assert!(response.status().is_success());
        let bytes = response.bytes().await.expect("读取响应失败");
        let sync_response = SyncResponse::decode(&*bytes).expect("解码失败");
        println!("从服务器下载: {} 条", sync_response.total);

        // 应用到客户端B
        let mut inserted = 0i32;
        let mut updated = 0i32;
        let mut skipped = 0i32;
        for item in &sync_response.items {
            match sync_b.apply_remote_update(item).expect("应用失败") {
                s if s == "inserted" => inserted += 1,
                s if s == "updated" => updated += 1,
                s if s == "skipped" => skipped += 1,
                _ => {}
            }
        }
        println!("客户端B: inserted={}, updated={}, skipped={}", inserted, updated, skipped);

        let count_b = sync_b.count().expect("计数失败");
        println!("客户端B总数: {}", count_b);

        // 验证一致性
        let items_a_final = sync_a.get_items().expect("查询失败");
        let items_b_final = sync_b.get_items().expect("查询失败");

        println!("客户端A记录数: {}", items_a_final.len());
        println!("客户端B记录数: {}", items_b_final.len());

        // 比较每条记录
        let mut mismatch_count = 0;
        for item_a in &items_a_final {
            let found = items_b_final.iter().find(|b| b.id == item_a.id);
            if let Some(item_b) = found {
                if item_a.kind != item_b.kind || item_a.item != item_b.item || item_a.data != item_b.data {
                    println!("不匹配: id={}, A=({},{},{}), B=({},{},{})", 
                        item_a.id, item_a.kind, item_a.item, item_a.data,
                        item_b.kind, item_b.item, item_b.data);
                    mismatch_count += 1;
                }
            } else {
                println!("客户端B缺少记录: id={}", item_a.id);
                mismatch_count += 1;
            }
        }

        println!("不匹配记录数: {}", mismatch_count);
        assert_eq!(mismatch_count, 0, "所有记录应该一致");
        
        println!("\n✅ 端到端同步测试通过！客户端A和客户端B数据一致");
    }

    #[tokio::test]
    async fn test_sync_with_update() {
        let client = Client::new();

        // 清理 - 使用独立的服务器数据库
        let server_db = "tmp/data/server_update.db";
        let _ = std::fs::remove_file(server_db);
        let _ = std::fs::remove_file("tmp/data/update_a.db");
        let _ = std::fs::remove_file("tmp/data/update_b.db");

        // 创建服务器数据库
        let server_sync = DataSync::with_remote_db(server_db);
        server_sync.ensure_table().expect("建表失败");

        // 客户端A：插入数据
        let sync_a = DataSync::with_remote_db("tmp/data/update_a.db");
        sync_a.ensure_table().expect("建表失败");

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "original".to_string(),
            item: "original_item".to_string(),
            data: "original_data".to_string(),
            upby: "client_a".to_string(),
            uptime: "2024-01-01 00:00:00".to_string(),
        };
        let id = sync_a.insert_item(&item).expect("插入失败");
        println!("插入记录: {}", id);

        // 上传到服务器数据库
        let items = sync_a.get_pending_items(10).expect("获取失败");
        for upload_item in &items {
            server_sync.apply_remote_update(upload_item).expect("写入服务器失败");
        }
        println!("上传到服务器: {} 条", items.len());
        
        // 标记已同步
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        sync_a.mark_synced(&ids).expect("标记同步失败");

        // 客户端A更新数据（更新uptime）
        let updated_item = testtbItem {
            id: id.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "updated".to_string(),
            item: "updated_item".to_string(),
            data: "updated_data".to_string(),
            upby: "client_a".to_string(),
            uptime: "2024-12-31 23:59:59".to_string(),
        };
        sync_a.update_item(&updated_item).expect("更新失败");
        println!("更新记录: {}", id);

        // 再次上传到服务器
        let items = sync_a.get_pending_items(10).expect("获取失败");
        println!("待上传记录数: {}", items.len());
        for upload_item in &items {
            server_sync.apply_remote_update(upload_item).expect("写入服务器失败");
        }
        println!("上传更新到服务器: {} 条", items.len());

        // 客户端B从服务器下载
        let sync_b = DataSync::with_remote_db("tmp/data/update_b.db");
        sync_b.ensure_table().expect("建表失败");

        let server_items = server_sync.get_items().expect("查询服务器失败");
        println!("服务器记录数: {}", server_items.len());

        for item in &server_items {
            println!("服务器记录: id={}, kind={}, item={}", item.id, item.kind, item.item);
            sync_b.apply_remote_update(item).expect("应用失败");
        }

        // 验证客户端B有最新数据
        let found = sync_b.get_item_by_id(&id).expect("查询失败").expect("未找到");
        assert_eq!(found.kind, "updated", "应该是更新后的数据");
        assert_eq!(found.item, "updated_item");
        assert_eq!(found.data, "updated_data");
        println!("客户端B数据: kind={}, item={}, data={}", found.kind, found.item, found.data);

        println!("\n✅ 更新同步测试通过！");
    }
}
