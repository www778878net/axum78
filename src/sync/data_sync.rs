//! DataSync - 数据同步服务
//!
//! 参考 logsvc 的同步机制：
//! 1. 本地变更记录到 sync_queue 表
//! 2. 上传：从 sync_queue 取数据发送到服务器
//! 3. 下载：从服务器获取数据，比较 uptime 后更新本地

use crate::proto::{testtb, testtbItem, SyncRequest, SyncResponse};
use base::{MyLogger, UpInfo};
use chrono::Local;
use database::Sqlite78;
use prost::Message;
use serde::{Deserialize, Serialize};

/// 同步配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub table_name: String,
    pub apiurl: String,
    pub cid: String,
    pub download_interval: i64,
    pub upload_interval: i64,
    pub getnumber: i32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            table_name: "testtb".to_string(),
            apiurl: String::new(),
            cid: "default".to_string(),
            download_interval: 300,
            upload_interval: 300,
            getnumber: 50,
        }
    }
}

/// 同步结果
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub inserted: i32,
    pub updated: i32,
    pub skipped: i32,
    pub total: i32,
}

/// DataSync - 数据同步服务
pub struct DataSync {
    pub config: SyncConfig,
    pub db: Sqlite78,
    pub logger: MyLogger,
    last_download: i64,
    last_upload: i64,
}

impl DataSync {
    pub fn new(config: SyncConfig, db: Sqlite78) -> Self {
        Self {
            config,
            db,
            logger: MyLogger::new("data_sync", 3),
            last_download: 0,
            last_upload: 0,
        }
    }

    pub fn with_remote_db(db_path: &str) -> Self {
        let mut db = Sqlite78::with_config(db_path, false, false);
        db.initialize().expect("初始化数据库失败");
        
        Self {
            config: SyncConfig::default(),
            db,
            logger: MyLogger::new("data_sync", 3),
            last_download: 0,
            last_upload: 0,
        }
    }

    pub fn set_table(&mut self, table_name: &str, apiurl: &str, cid: &str) {
        self.config.table_name = table_name.to_string();
        self.config.apiurl = apiurl.to_string();
        self.config.cid = cid.to_string();
    }

    /// 初始化表结构（数据表 + sync_queue）
    pub fn ensure_table(&self) -> Result<(), String> {
        let table_sql = format!(
            r#"CREATE TABLE IF NOT EXISTS {} (
                idpk INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL,
                cid TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT '',
                item TEXT NOT NULL DEFAULT '',
                data TEXT NOT NULL DEFAULT '',
                upby TEXT NOT NULL DEFAULT '',
                uptime TEXT NOT NULL DEFAULT '',
                UNIQUE(id)
            )"#,
            self.config.table_name
        );

        let up = UpInfo::new();
        self.db.do_m(&table_sql, &[], &up)?;
        
        // 检查 sync_queue 表结构，如果不存在 created_at 列则重建
        let check_sql = "PRAGMA table_info(sync_queue)";
        let rows = self.db.do_get(check_sql, &[], &up).unwrap_or_default();
        let has_created_at = rows.iter().any(|r| {
            r.get("name").and_then(|v| v.as_str()).unwrap_or("") == "created_at"
        });
        
        if !has_created_at {
            // 删除旧表重建
            let _ = self.db.do_m("DROP TABLE IF EXISTS sync_queue", &[], &up);
        }
        
        let queue_sql = r#"CREATE TABLE IF NOT EXISTS sync_queue (
            idpk INTEGER PRIMARY KEY AUTOINCREMENT,
            table_name TEXT NOT NULL,
            id TEXT NOT NULL,
            action TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '',
            synced INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT ''
        )"#;
        
        self.db.do_m(queue_sql, &[], &up)?;
        
        self.logger.detail(&format!("表 {} 和 sync_queue 创建/验证成功", self.config.table_name));
        Ok(())
    }

    /// 插入记录并记录到 sync_queue
    pub fn insert_item(&self, item: &testtbItem) -> Result<String, String> {
        let sql = format!(
            "INSERT INTO {} (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)",
            self.config.table_name
        );

        let up = UpInfo::new();

        let id = if item.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            item.id.clone()
        };

        let uptime = if item.uptime.is_empty() {
            Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            item.uptime.clone()
        };

        self.db.do_m_add(
            &sql,
            &[
                &id as &dyn rusqlite::ToSql,
                &item.cid,
                &item.kind,
                &item.item,
                &item.data,
                &item.upby,
                &uptime,
            ],
            &up,
        )?;

        self.add_to_sync_queue(&id, "insert", &serde_json::to_string(item).unwrap_or_default())?;
        
        self.logger.detail(&format!("插入记录并加入同步队列: {}", id));
        Ok(id)
    }

    /// 更新记录并记录到 sync_queue
    pub fn update_item(&self, item: &testtbItem) -> Result<bool, String> {
        if item.id.is_empty() {
            return Err("id 不能为空".to_string());
        }

        let sql = format!(
            "UPDATE {} SET kind = ?, item = ?, data = ?, upby = ?, uptime = ? WHERE id = ? AND cid = ?",
            self.config.table_name
        );

        let up = UpInfo::new();
        let uptime = if item.uptime.is_empty() {
            Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            item.uptime.clone()
        };

        let result = self.db.do_m(
            &sql,
            &[
                &item.kind as &dyn rusqlite::ToSql,
                &item.item,
                &item.data,
                &item.upby,
                &uptime,
                &item.id,
                &item.cid,
            ],
            &up,
        )?;

        let updated = result.affected_rows > 0;
        if updated {
            self.add_to_sync_queue(&item.id, "update", &serde_json::to_string(item).unwrap_or_default())?;
        }
        
        self.logger.detail(&format!("更新记录并加入同步队列: {} -> {}", item.id, updated));
        Ok(updated)
    }

    /// 删除记录并记录到 sync_queue
    pub fn delete_item(&self, id: &str) -> Result<bool, String> {
        if id.is_empty() {
            return Err("id 不能为空".to_string());
        }

        let sql = format!(
            "DELETE FROM {} WHERE id = ? AND cid = ?",
            self.config.table_name
        );

        let up = UpInfo::new();
        let result = self.db.do_m(&sql, &[&id as &dyn rusqlite::ToSql, &self.config.cid], &up)?;

        let deleted = result.affected_rows > 0;
        if deleted {
            self.add_to_sync_queue(id, "delete", "")?;
        }
        
        self.logger.detail(&format!("删除记录并加入同步队列: {} -> {}", id, deleted));
        Ok(deleted)
    }

    /// 添加到同步队列
    fn add_to_sync_queue(&self, id: &str, action: &str, data: &str) -> Result<(), String> {
        let sql = "INSERT INTO sync_queue (table_name, id, action, data, synced, created_at) VALUES (?, ?, ?, ?, 0, ?)";
        let up = UpInfo::new();
        let created_at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        self.db.do_m(
            sql,
            &[
                &self.config.table_name as &dyn rusqlite::ToSql,
                &id,
                &action,
                &data,
                &created_at,
            ],
            &up,
        )?;
        
        Ok(())
    }

    /// 获取待同步的记录数
    pub fn get_pending_count(&self) -> Result<i32, String> {
        let up = UpInfo::new();
        let sql = "SELECT COUNT(*) as cnt FROM sync_queue WHERE table_name = ? AND synced = 0";
        let rows = self.db.do_get(&sql, &[&self.config.table_name as &dyn rusqlite::ToSql], &up)?;
        
        if let Some(row) = rows.first() {
            Ok(row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
        } else {
            Ok(0)
        }
    }

    /// 获取待同步的记录
    pub fn get_pending_items(&self, limit: i32) -> Result<Vec<testtbItem>, String> {
        let up = UpInfo::new();
        let sql = format!(
            "SELECT t.* FROM {} t INNER JOIN sync_queue q ON t.id = q.id WHERE q.table_name = ? AND q.synced = 0 AND q.action != 'delete' ORDER BY q.idpk ASC LIMIT ?",
            self.config.table_name
        );
        
        let rows = self.db.do_get(&sql, &[&self.config.table_name as &dyn rusqlite::ToSql, &limit as &dyn rusqlite::ToSql], &up)?;

        let items: Vec<testtbItem> = rows
            .iter()
            .map(|row| testtbItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                kind: row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                item: row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                data: row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                uptime: row.get("uptime").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .collect();

        Ok(items)
    }

    /// 标记已同步
    pub fn mark_synced(&self, ids: &[String]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        
        let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
        let sql = format!("UPDATE sync_queue SET synced = 1 WHERE table_name = ? AND id IN ({})", placeholders.join(","));
        
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&self.config.table_name];
        for id in ids {
            params.push(id);
        }
        
        let up = UpInfo::new();
        self.db.do_m(&sql, &params, &up)?;
        
        Ok(())
    }

    /// 获取所有记录
    pub fn get_items(&self) -> Result<Vec<testtbItem>, String> {
        let up = UpInfo::new();

        let sql = format!(
            "SELECT * FROM {} WHERE cid = ? ORDER BY idpk DESC LIMIT 100",
            self.config.table_name
        );

        let rows = self.db.do_get(&sql, &[&self.config.cid as &dyn rusqlite::ToSql], &up)?;

        let items: Vec<testtbItem> = rows
            .iter()
            .map(|row| testtbItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                kind: row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                item: row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                data: row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                uptime: row.get("uptime").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .collect();

        Ok(items)
    }

    /// 根据 ID 获取记录
    pub fn get_item_by_id(&self, id: &str) -> Result<Option<testtbItem>, String> {
        let up = UpInfo::new();

        let sql = format!(
            "SELECT * FROM {} WHERE id = ? AND cid = ? LIMIT 1",
            self.config.table_name
        );

        let rows = self.db.do_get(&sql, &[&id as &dyn rusqlite::ToSql, &self.config.cid], &up)?;

        if let Some(row) = rows.first() {
            Ok(Some(testtbItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                kind: row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                item: row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                data: row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                upby: row.get("upby").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                uptime: row.get("uptime").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    /// 统计记录数
    pub fn count(&self) -> Result<i32, String> {
        let up = UpInfo::new();
        let sql = format!("SELECT COUNT(*) as cnt FROM {} WHERE cid = ?", self.config.table_name);
        let rows = self.db.do_get(&sql, &[&self.config.cid as &dyn rusqlite::ToSql], &up)?;
        
        if let Some(row) = rows.first() {
            Ok(row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
        } else {
            Ok(0)
        }
    }

    /// 编码为 protobuf
    pub fn encode_items(&self, items: &[testtbItem]) -> Vec<u8> {
        let msg = testtb {
            sid: String::new(),
            items: items.to_vec(),
        };
        msg.encode_to_vec()
    }

    /// 解码 protobuf
    pub fn decode_items(data: &[u8]) -> Result<Vec<testtbItem>, String> {
        let msg = testtb::decode(data)
            .map_err(|e| format!("解码失败: {}", e))?;
        Ok(msg.items)
    }

    /// 应用远程更新（下载时使用）
    /// 比较 uptime，决定插入/更新/跳过
    pub fn apply_remote_update(&self, item: &testtbItem) -> Result<String, String> {
        let existing = self.get_item_by_id(&item.id)?;
        
        match existing {
            None => {
                let sql = format!(
                    "INSERT INTO {} (id, cid, kind, item, data, upby, uptime) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    self.config.table_name
                );
                let up = UpInfo::new();
                self.db.do_m_add(
                    &sql,
                    &[
                        &item.id as &dyn rusqlite::ToSql,
                        &item.cid,
                        &item.kind,
                        &item.item,
                        &item.data,
                        &item.upby,
                        &item.uptime,
                    ],
                    &up,
                )?;
                self.logger.detail(&format!("远程数据插入: {}", item.id));
                Ok("inserted".to_string())
            }
            Some(local) => {
                if item.uptime > local.uptime {
                    let sql = format!(
                        "UPDATE {} SET kind = ?, item = ?, data = ?, upby = ?, uptime = ? WHERE id = ?",
                        self.config.table_name
                    );
                    let up = UpInfo::new();
                    self.db.do_m(
                        &sql,
                        &[
                            &item.kind as &dyn rusqlite::ToSql,
                            &item.item,
                            &item.data,
                            &item.upby,
                            &item.uptime,
                            &item.id,
                        ],
                        &up,
                    )?;
                    self.logger.detail(&format!("远程数据更新: {}", item.id));
                    Ok("updated".to_string())
                } else {
                    self.logger.detail(&format!("远程数据跳过(本地更新): {}", item.id));
                    Ok("skipped".to_string())
                }
            }
        }
    }

    /// 上传到服务器
    pub async fn upload_to_server(&self) -> Result<SyncResult, String> {
        if self.config.apiurl.is_empty() {
            return Err("apiurl 未配置".to_string());
        }

        let pending_count = self.get_pending_count()?;
        if pending_count == 0 {
            self.logger.detail("没有待同步的数据");
            return Ok(SyncResult::default());
        }

        let items = self.get_pending_items(50)?;
        if items.is_empty() {
            return Ok(SyncResult::default());
        }

        let encoded = self.encode_items(&items);
        
        let client = reqwest::Client::new();
        let url = format!("{}/sync/{}", self.config.apiurl, self.config.table_name);
        
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(encoded)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("服务器错误: {}", response.status()));
        }

        let bytes = response.bytes().await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        let sync_response = SyncResponse::decode(&*bytes)
            .map_err(|e| format!("解码响应失败: {}", e))?;

        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        self.mark_synced(&ids)?;

        self.logger.detail(&format!("上传成功: {} 条", sync_response.total));

        Ok(SyncResult {
            inserted: sync_response.total,
            updated: 0,
            skipped: 0,
            total: sync_response.total,
        })
    }

    /// 从服务器下载
    pub async fn download_from_server(&mut self) -> Result<SyncResult, String> {
        if self.config.apiurl.is_empty() {
            return Err("apiurl 未配置".to_string());
        }

        let request = SyncRequest {
            table_name: self.config.table_name.clone(),
            sid: String::new(),
            cid: self.config.cid.clone(),
            getstart: 0,
            getnumber: self.config.getnumber,
            last_uptime: String::new(),
        };

        let encoded = request.encode_to_vec();
        
        let client = reqwest::Client::new();
        let url = format!("{}/sync/{}/get", self.config.apiurl, self.config.table_name);
        
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(encoded)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("服务器错误: {}", response.status()));
        }

        let bytes = response.bytes().await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        let sync_response = SyncResponse::decode(&*bytes)
            .map_err(|e| format!("解码响应失败: {}", e))?;

        let mut result = SyncResult::default();
        
        for item in &sync_response.items {
            match self.apply_remote_update(item)? {
                s if s == "inserted" => result.inserted += 1,
                s if s == "updated" => result.updated += 1,
                s if s == "skipped" => result.skipped += 1,
                _ => {}
            }
            result.total += 1;
        }

        self.logger.detail(&format!(
            "下载完成: inserted={}, updated={}, skipped={}",
            result.inserted, result.updated, result.skipped
        ));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sync() -> DataSync {
        let sync = DataSync::with_remote_db("tmp/data/remote.db");
        sync.ensure_table().expect("建表失败");
        sync
    }

    #[test]
    fn test_ensure_table() {
        let sync = create_test_sync();
        sync.count().expect("计数失败");
    }

    #[test]
    fn test_insert_with_sync_queue() {
        let sync = create_test_sync();

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "sync_queue_test".to_string(),
            item: "test_item".to_string(),
            data: "test_data".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        let id = sync.insert_item(&item).expect("插入失败");
        assert!(!id.is_empty());

        let pending = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending > 0, "应该有待同步记录");
    }

    #[test]
    fn test_update_with_sync_queue() {
        let sync = create_test_sync();

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "update_before".to_string(),
            item: "update_item".to_string(),
            data: "update_data".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        let id = sync.insert_item(&item).expect("插入失败");

        let update_item = testtbItem {
            id: id.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "update_after".to_string(),
            item: "updated_item".to_string(),
            data: "updated_data".to_string(),
            upby: "tester2".to_string(),
            uptime: String::new(),
        };

        let updated = sync.update_item(&update_item).expect("更新失败");
        assert!(updated);

        let pending = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending >= 2, "insert + update 应该有2条待同步记录");
    }

    #[test]
    fn test_delete_with_sync_queue() {
        let sync = create_test_sync();

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "delete_test".to_string(),
            item: "delete_item".to_string(),
            data: "delete_data".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        let id = sync.insert_item(&item).expect("插入失败");
        
        let pending_before = sync.get_pending_count().expect("获取待同步数失败");

        let deleted = sync.delete_item(&id).expect("删除失败");
        assert!(deleted);

        let pending_after = sync.get_pending_count().expect("获取待同步数失败");
        assert!(pending_after > pending_before, "删除后应该增加待同步记录");
    }

    #[test]
    fn test_apply_remote_update_insert() {
        let sync = create_test_sync();

        let item = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "remote_insert".to_string(),
            item: "remote_item".to_string(),
            data: "remote_data".to_string(),
            upby: "remote".to_string(),
            uptime: "2024-01-01 00:00:00".to_string(),
        };

        let result = sync.apply_remote_update(&item).expect("应用远程更新失败");
        assert_eq!(result, "inserted");

        let found = sync.get_item_by_id(&item.id).expect("查询失败").expect("未找到");
        assert_eq!(found.kind, "remote_insert");
    }

    #[test]
    fn test_apply_remote_update_skip() {
        let sync = create_test_sync();

        let item = testtbItem {
            id: uuid::Uuid::new_v4().to_string(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "local_newer".to_string(),
            item: "local_item".to_string(),
            data: "local_data".to_string(),
            upby: "local".to_string(),
            uptime: "2024-12-31 23:59:59".to_string(),
        };

        sync.apply_remote_update(&item).expect("插入失败");

        let remote_item = testtbItem {
            id: item.id.clone(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "remote_older".to_string(),
            item: "remote_item".to_string(),
            data: "remote_data".to_string(),
            upby: "remote".to_string(),
            uptime: "2024-01-01 00:00:00".to_string(),
        };

        let result = sync.apply_remote_update(&remote_item).expect("应用远程更新失败");
        assert_eq!(result, "skipped");

        let found = sync.get_item_by_id(&item.id).expect("查询失败").expect("未找到");
        assert_eq!(found.kind, "local_newer", "应该保持本地更新的数据");
    }

    #[test]
    fn test_mark_synced() {
        let _ = std::fs::remove_file("tmp/data/mark_synced_test.db");
        let sync = DataSync::with_remote_db("tmp/data/mark_synced_test.db");
        sync.ensure_table().expect("建表失败");

        let item = testtbItem {
            id: String::new(),
            idpk: 0,
            cid: "default".to_string(),
            kind: "mark_synced_test".to_string(),
            item: "test_item".to_string(),
            data: "test_data".to_string(),
            upby: "tester".to_string(),
            uptime: String::new(),
        };

        let id = sync.insert_item(&item).expect("插入失败");
        
        let pending_before = sync.get_pending_count().expect("获取待同步数失败");
        assert_eq!(pending_before, 1, "插入后应有1条待同步记录");

        sync.mark_synced(&[id]).expect("标记同步失败");

        let pending_after = sync.get_pending_count().expect("获取待同步数失败");
        assert_eq!(pending_after, 0, "标记后应没有待同步记录");
    }

    #[test]
    fn test_protobuf_encode_decode() {
        let items = vec![
            testtbItem {
                id: "test-id-1".to_string(),
                idpk: 1,
                cid: "default".to_string(),
                kind: "kind1".to_string(),
                item: "item1".to_string(),
                data: "data1".to_string(),
                upby: "tester".to_string(),
                uptime: "2024-01-01 00:00:00".to_string(),
            },
        ];

        let sync = create_test_sync();
        let encoded = sync.encode_items(&items);
        assert!(!encoded.is_empty());

        let decoded = DataSync::decode_items(&encoded).expect("解码失败");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].id, "test-id-1");
    }
}
