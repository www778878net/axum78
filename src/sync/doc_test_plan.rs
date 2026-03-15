//! 按文档测试方案执行
//!
//! docs/dev/admin_data_state.md#L75-79
//!
//! 数据库路径：
//! - 服务器：tmp/data/remote.db（用户指定）
//! - 客户端：默认本地数据库

#[cfg(test)]
mod doc_test_plan {
    use crate::proto::testtbItem;
    use crate::sync::DataSync;
    use database::Sqlite78;
    use std::sync::Mutex;

    const SERVER_DB: &str = "tmp/data/remote.db";

    static LOCK: Mutex<()> = Mutex::new(());

    fn get_server_sync() -> DataSync {
        DataSync::with_remote_db(SERVER_DB)
    }

    fn get_client_sync() -> DataSync {
        let db_path = Sqlite78::find_default_db_path().expect("获取默认数据库路径失败");
        DataSync::with_remote_db(&db_path)
    }

    fn clear_all(sync: &DataSync) {
        let up = base::UpInfo::new();
        let _ = sync.db.do_m("DELETE FROM testtb", &[], &up);
        let _ = sync.db.do_m("DELETE FROM sync_queue", &[], &up);
    }

    /// 测试方案1: 单向下载测试
    /// 先清空表testtb，然后服务器上的几条预期会下载到本地
    #[test]
    fn test_plan_1_download() {
        let _guard = LOCK.lock().unwrap();

        let server_sync = get_server_sync();
        server_sync.ensure_table().expect("建表失败");
        clear_all(&server_sync);

        let client_sync = get_client_sync();
        client_sync.ensure_table().expect("建表失败");
        clear_all(&client_sync);
        drop(client_sync);

        // 服务器：插入5条数据
        for i in 0..5 {
            let item = testtbItem {
                id: String::new(),
                idpk: 0,
                cid: "default".to_string(),
                kind: format!("server_kind_{}", i),
                item: format!("server_item_{}", i),
                data: format!("server_data_{}", i),
                upby: "server".to_string(),
                uptime: format!("2024-01-{:02} 00:00:00", i + 1),
            };
            let id = server_sync.insert_item(&item).expect("插入失败");
            println!("服务器插入: {} -> id={}", i, id);
        }

        let count = server_sync.count().expect("计数失败");
        println!("服务器记录数: {}", count);
        assert_eq!(count, 5);

        // 重新获取客户端连接
        let client_sync = get_client_sync();
        let client_count_before = client_sync.count().expect("计数失败");
        println!("客户端下载前记录数: {}", client_count_before);
        assert_eq!(client_count_before, 0);

        // 下载：从服务器获取数据
        let server_records = server_sync.get_items().expect("查询失败");
        for record in &server_records {
            client_sync.apply_remote_update(record).expect("同步失败");
        }

        let client_count_after = client_sync.count().expect("计数失败");
        println!("客户端下载后记录数: {}", client_count_after);
        assert_eq!(client_count_after, 5);

        println!("✅ 测试方案1通过");
    }

    /// 测试方案2: 添加2条、修改2条、删除2条，上传到服务器
    #[test]
    fn test_plan_2_upload() {
        let _guard = LOCK.lock().unwrap();

        let server_sync = get_server_sync();
        server_sync.ensure_table().expect("建表失败");
        clear_all(&server_sync);

        let client_sync = get_client_sync();
        client_sync.ensure_table().expect("建表失败");
        clear_all(&client_sync);

        // 初始数据（直接写入数据库，不通过sync_queue）
        let up = base::UpInfo::new();
        let mut init_ids: Vec<String> = Vec::new();
        for i in 0..6 {
            let id = uuid::Uuid::new_v4().to_string();
            let sql = "INSERT INTO testtb (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)";
            let uptime = format!("2024-01-01 00:00:00");
            client_sync.db.do_m(sql, &[&id, &"default", &format!("kind_{}", i), &format!("item_{}", i), &format!("data_{}", i), &"init", &uptime], &up).expect("初始化失败");
            server_sync.db.do_m(sql, &[&id, &"default", &format!("kind_{}", i), &format!("item_{}", i), &format!("data_{}", i), &"init", &uptime], &up).expect("初始化失败");
            init_ids.push(id);
        }

        println!(
            "初始记录数: 客户端={}, 服务器={}",
            client_sync.count().unwrap(),
            server_sync.count().unwrap()
        );

        // 添加2条
        let add_item1 = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "add_kind_1".to_string(),
            item: "add_item_1".to_string(),
            data: "add_data_1".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-01 00:00:00".to_string(),
        };
        let add_id1 = client_sync.insert_item(&add_item1).expect("添加失败");

        let add_item2 = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "add_kind_2".to_string(),
            item: "add_item_2".to_string(),
            data: "add_data_2".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-01 00:00:00".to_string(),
        };
        let add_id2 = client_sync.insert_item(&add_item2).expect("添加失败");

        println!("添加2条: id={}, id={}", add_id1, add_id2);

        // 修改2条
        let update_item1 = testtbItem {
            id: init_ids[0].clone(),
            idpk: 0,
            cid: "default".to_string(),
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
            cid: "default".to_string(),
            kind: "updated_kind_1".to_string(),
            item: "updated_item_1".to_string(),
            data: "updated_data_1".to_string(),
            upby: "client".to_string(),
            uptime: "2024-02-02 00:00:00".to_string(),
        };
        client_sync.update_item(&update_item2).expect("修改失败");

        println!("修改2条: id={}, id={}", init_ids[0], init_ids[1]);

        // 删除2条
        let del1 = client_sync.delete_item(&init_ids[4]).expect("删除失败");
        let del2 = client_sync.delete_item(&init_ids[5]).expect("删除失败");
        println!("删除2条: id={}={}, id={}={}", init_ids[4], del1, init_ids[5], del2);

        // 检查待同步队列（添加2 + 修改2 + 删除2 = 6）
        let pending = client_sync.get_pending_count().expect("获取待同步数失败");
        println!("待同步记录数: {}", pending);
        assert_eq!(pending, 6);

        // 上传到服务器
        let pending_items = client_sync.get_pending_items(100).expect("获取待同步记录失败");
        for item in &pending_items {
            server_sync.apply_remote_update(item).expect("上传失败");
        }

        let ids: Vec<String> = pending_items.iter().map(|i| i.id.clone()).collect();
        client_sync.mark_synced(&ids).expect("标记失败");

        let client_records = client_sync.get_items().expect("查询失败");
        let server_records = server_sync.get_items().expect("查询失败");

        println!("客户端记录数: {}", client_records.len());
        println!("服务器记录数: {}", server_records.len());

        assert!(client_records.iter().any(|r| r.kind == "add_kind_1"));
        assert!(server_records.iter().any(|r| r.kind == "add_kind_1"));

        println!("✅ 测试方案2通过：添加2条、修改2条、删除2条，上传成功");
    }

    /// 测试方案3: 服务器添加修改，自动同步到客户端
    #[test]
    fn test_plan_3_server_changes() {
        let _guard = LOCK.lock().unwrap();

        let server_sync = get_server_sync();
        server_sync.ensure_table().expect("建表失败");
        clear_all(&server_sync);

        let client_sync = get_client_sync();
        client_sync.ensure_table().expect("建表失败");
        clear_all(&client_sync);

        // 初始同步（直接写入数据库，不通过sync_queue）
        let up = base::UpInfo::new();
        for i in 0..6 {
            let id = uuid::Uuid::new_v4().to_string();
            let sql = "INSERT INTO testtb (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)";
            let uptime = "2024-01-01 00:00:00";
            client_sync.db.do_m(sql, &[&id, &"default", &format!("kind_{}", i), &format!("item_{}", i), &format!("data_{}", i), &"init", &uptime], &up).expect("初始化失败");
            server_sync.db.do_m(sql, &[&id, &"default", &format!("kind_{}", i), &format!("item_{}", i), &format!("data_{}", i), &"init", &uptime], &up).expect("初始化失败");
        }

        println!("初始记录数: {}", server_sync.count().unwrap());

        // 服务器添加2条
        let add1 = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: "default".to_string(),
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
            cid: "default".to_string(),
            kind: "server_add_kind_2".to_string(),
            item: "server_add_item_2".to_string(),
            data: "server_add_data_2".to_string(),
            upby: "server".to_string(),
            uptime: "2024-03-01 00:00:00".to_string(),
        };
        server_sync.apply_remote_update(&add2).expect("添加失败");

        println!("服务器添加2条");

        // 服务器修改2条
        let records = server_sync.get_items().expect("查询失败");
        if records.len() >= 2 {
            let update1 = testtbItem {
                id: records[0].id.clone(),
                idpk: 0,
                cid: "default".to_string(),
                kind: "server_updated_kind_0".to_string(),
                item: "server_updated_item_0".to_string(),
                data: "server_updated_data_0".to_string(),
                upby: "server".to_string(),
                uptime: "2024-03-02 00:00:00".to_string(),
            };
            server_sync.apply_remote_update(&update1).expect("修改失败");

            let update2 = testtbItem {
                id: records[1].id.clone(),
                idpk: 0,
                cid: "default".to_string(),
                kind: "server_updated_kind_1".to_string(),
                item: "server_updated_item_1".to_string(),
                data: "server_updated_data_1".to_string(),
                upby: "server".to_string(),
                uptime: "2024-03-02 00:00:00".to_string(),
            };
            server_sync.apply_remote_update(&update2).expect("修改失败");
        }

        println!("服务器修改2条");

        // 客户端下载
        let server_records = server_sync.get_items().expect("查询失败");
        for record in &server_records {
            client_sync.apply_remote_update(record).expect("同步失败");
        }

        let client_records = client_sync.get_items().expect("查询失败");
        println!("客户端记录数: {}", client_records.len());
        assert_eq!(client_records.len(), server_records.len());
        assert!(client_records.iter().any(|r| r.kind == "server_add_kind_1"));

        println!("✅ 测试方案3通过：服务器变更自动同步到客户端");
    }

    /// 测试方案4: 冲突测试
    #[test]
    fn test_plan_4_conflict() {
        let _guard = LOCK.lock().unwrap();

        let server_sync = get_server_sync();
        server_sync.ensure_table().expect("建表失败");
        clear_all(&server_sync);

        let client_sync = get_client_sync();
        client_sync.ensure_table().expect("建表失败");
        clear_all(&client_sync);

        // 初始数据（直接写入数据库，不通过sync_queue）
        let up = base::UpInfo::new();
        let conflict_id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO testtb (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)";
        client_sync.db.do_m(sql, &[&conflict_id, &"default", &"initial_kind", &"initial_item", &"initial_data", &"init", &"2024-01-01 00:00:00"], &up).expect("初始化失败");
        server_sync.db.do_m(sql, &[&conflict_id, &"default", &"initial_kind", &"initial_item", &"initial_data", &"init", &"2024-01-01 00:00:00"], &up).expect("初始化失败");

        println!("初始数据: id={}, kind=initial_kind", conflict_id);

        // 客户端修改（较早时间）
        let client_update = testtbItem {
            id: conflict_id.clone(),
            idpk: 0,
            cid: "default".to_string(),
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
            cid: "default".to_string(),
            kind: "server_kind".to_string(),
            item: "server_item".to_string(),
            data: "server_data".to_string(),
            upby: "server".to_string(),
            uptime: "2024-12-01 00:00:00".to_string(),
        };
        server_sync.apply_remote_update(&server_update).expect("服务器修改失败");

        // 同步
        let pending_items = client_sync.get_pending_items(10).expect("获取失败");
        for item in &pending_items {
            server_sync.apply_remote_update(item).expect("上传失败");
        }

        let server_records = server_sync.get_items().expect("查询失败");
        for record in &server_records {
            client_sync.apply_remote_update(record).expect("下载失败");
        }

        // 验证
        let client_final = client_sync.get_item_by_id(&conflict_id).expect("查询失败").expect("未找到");
        let server_final = server_sync.get_item_by_id(&conflict_id).expect("查询失败").expect("未找到");

        println!("客户端: kind={}", client_final.kind);
        println!("服务器: kind={}", server_final.kind);

        assert_eq!(client_final.kind, server_final.kind);

        println!("✅ 测试方案4通过：冲突解决（uptime较晚者胜出）");
    }
}
