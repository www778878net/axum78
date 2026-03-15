//! 测试同步客户端
//!
//! 运行: cargo test -p axum78 sync_client -- --nocapture

#[cfg(test)]
mod sync_client_tests {
    use crate::proto::{testtb, testtbItem, SyncRequest, SyncResponse};
    use crate::sync::DataSync;
    use prost::Message;
    use reqwest::Client;

    const SERVER_URL: &str = "http://127.0.0.1:3780";

    #[tokio::test]
    async fn test_upload_pending_to_server() {
        let client = Client::new();

        let mut sync = DataSync::with_remote_db("tmp/data/client.db");
        sync.ensure_table().expect("建表失败");

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "pending_upload".to_string(),
            item: "pending_item".to_string(),
            data: "pending_data".to_string(),
            upby: "test_client".to_string(),
            uptime: String::new(),
        };

        sync.insert_item(&item).expect("插入失败");

        let pending = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending > 0, "应该有待同步记录");

        let items = sync.get_pending_items(50).expect("获取待同步记录失败");
        assert!(!items.is_empty());

        let msg = testtb { items };
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

        println!("上传成功: {} 条记录", sync_response.total);
        assert_eq!(sync_response.res, 0);
    }

    #[tokio::test]
    async fn test_download_and_apply() {
        let client = Client::new();

        let request = SyncRequest {
            table_name: "testtb".to_string(),
            cid: "default".to_string(),
            getstart: 0,
            getnumber: 100,
            last_uptime: String::new(),
        };

        let encoded = request.encode_to_vec();

        let response = client
            .post(&format!("{}/sync/testtb/get", SERVER_URL))
            .header("Content-Type", "application/octet-stream")
            .body(encoded)
            .send()
            .await
            .expect("请求失败");

        assert!(response.status().is_success());

        let bytes = response.bytes().await.expect("读取响应失败");
        let sync_response = SyncResponse::decode(&*bytes).expect("解码失败");

        println!("下载成功: {} 条记录", sync_response.total);
        assert_eq!(sync_response.res, 0);

        let mut local_sync = DataSync::with_remote_db("tmp/data/client_download.db");
        local_sync.ensure_table().expect("建表失败");

        for item in &sync_response.items {
            local_sync.apply_remote_update(item).expect("应用远程更新失败");
        }

        let count = local_sync.count().expect("计数失败");
        println!("本地记录数: {}", count);
    }

    #[test]
    fn test_sync_queue_flow() {
        let sync = DataSync::with_remote_db("tmp/data/sync_queue_flow.db");
        sync.ensure_table().expect("建表失败");

        let item1 = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "flow_test_1".to_string(),
            item: "item1".to_string(),
            data: "data1".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        let id1 = sync.insert_item(&item1).expect("插入失败");

        let pending1 = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending1 >= 1, "插入后应有待同步记录");

        let update_item = testtbItem {
            id: id1.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "flow_test_updated".to_string(),
            item: "updated_item".to_string(),
            data: "updated_data".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        sync.update_item(&update_item).expect("更新失败");

        let pending2 = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending2 >= 2, "更新后应增加待同步记录");

        sync.mark_synced(&[id1.clone()]).expect("标记同步失败");

        println!("同步队列流程测试通过!");
    }

    #[test]
    fn test_uptime_compare() {
        let sync = DataSync::with_remote_db("tmp/data/uptime_test.db");
        sync.ensure_table().expect("建表失败");

        let local_item = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "local_newer".to_string(),
            item: "local_item".to_string(),
            data: "local_data".to_string(),
            upby: "local".to_string(),
            uptime: "2024-12-31 23:59:59".to_string(),
        };

        sync.apply_remote_update(&local_item).expect("插入失败");

        let older_remote = testtbItem {
            id: local_item.id.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "remote_older".to_string(),
            item: "remote_item".to_string(),
            data: "remote_data".to_string(),
            upby: "remote".to_string(),
            uptime: "2024-01-01 00:00:00".to_string(),
        };

        let result = sync.apply_remote_update(&older_remote).expect("应用远程更新失败");
        assert_eq!(result, "skipped", "远程数据较旧应该跳过");

        let found = sync.get_item_by_id(&local_item.id).expect("查询失败").expect("未找到");
        assert_eq!(found.kind, "local_newer", "应该保持本地更新的数据");

        let newer_remote = testtbItem {
            id: local_item.id.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "remote_newer".to_string(),
            item: "newer_remote_item".to_string(),
            data: "newer_remote_data".to_string(),
            upby: "remote".to_string(),
            uptime: "2025-01-01 00:00:00".to_string(),
        };

        let result = sync.apply_remote_update(&newer_remote).expect("应用远程更新失败");
        assert_eq!(result, "updated", "远程数据较新应该更新");

        let found = sync.get_item_by_id(&local_item.id).expect("查询失败").expect("未找到");
        assert_eq!(found.kind, "remote_newer", "应该更新为远程数据");

        println!("uptime 比较测试通过!");
    }
}
