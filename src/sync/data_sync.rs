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
            cid: String::new(),
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
}

impl DataSync {
    pub fn new(config: SyncConfig, db: Sqlite78) -> Self {
        Self {
            config,
            db,
            logger: MyLogger::new("data_sync", 3),
        }
    }

    pub fn with_remote_db(db_path: &str) -> Self {
        let mut db = Sqlite78::with_config(db_path, false, false);
        db.initialize().expect("初始化数据库失败");
        
        Self {
            config: SyncConfig::default(),
            db,
            logger: MyLogger::new("data_sync", 3),
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
                id TEXT NOT NULL UNIQUE,
                cid TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT '',
                item TEXT NOT NULL DEFAULT '',
                data TEXT NOT NULL DEFAULT ''
            )"#,
            self.config.table_name
        );

        let up = UpInfo::new();
        self.db.do_m(&table_sql, &[], &up)?;
        
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
            "INSERT INTO {} (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)",
            self.config.table_name
        );

        let up = UpInfo::new();

        let id = if item.id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            item.id.clone()
        };

        self.db.do_m_add(
            &sql,
            &[
                &id as &dyn rusqlite::ToSql,
                &item.cid,
                &item.kind,
                &item.item,
                &item.data,
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
            "UPDATE {} SET kind = ?, item = ?, data = ? WHERE id = ? AND cid = ?",
            self.config.table_name
        );

        let up = UpInfo::new();

        let result = self.db.do_m(
            &sql,
            &[
                &item.kind as &dyn rusqlite::ToSql,
                &item.item,
                &item.data,
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
            "DELETE FROM {} WHERE id = ?",
            self.config.table_name
        );

        let up = UpInfo::new();
        let result = self.db.do_m(&sql, &[&id as &dyn rusqlite::ToSql], &up)?;

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

    /// 获取所有记录（不过滤CID）
    pub fn get_items(&self) -> Result<Vec<testtbItem>, String> {
        let up = UpInfo::new();

        let sql = format!(
            "SELECT * FROM {} ORDER BY idpk DESC LIMIT 100",
            self.config.table_name
        );

        let rows = self.db.do_get(&sql, &[], &up)?;

        let items: Vec<testtbItem> = rows
            .iter()
            .map(|row| testtbItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                kind: row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                item: row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                data: row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .collect();

        Ok(items)
    }

    /// 根据 ID 获取记录
    pub fn get_item_by_id(&self, id: &str) -> Result<Option<testtbItem>, String> {
        let up = UpInfo::new();

        let sql = format!(
            "SELECT * FROM {} WHERE id = ? LIMIT 1",
            self.config.table_name
        );

        let rows = self.db.do_get(&sql, &[&id as &dyn rusqlite::ToSql], &up)?;

        if let Some(row) = rows.first() {
            Ok(Some(testtbItem {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                cid: row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                kind: row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                item: row.get("item").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                data: row.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    /// 统计记录数（不过滤CID）
    pub fn count(&self) -> Result<i32, String> {
        let up = UpInfo::new();
        let sql = format!("SELECT COUNT(*) as cnt FROM {}", self.config.table_name);
        let rows = self.db.do_get(&sql, &[], &up)?;
        
        if let Some(row) = rows.first() {
            Ok(row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
        } else {
            Ok(0)
        }
    }

    /// 编码为 protobuf
    pub fn encode_items(&self, items: &[testtbItem]) -> Vec<u8> {
        let msg = testtb {
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
    pub fn apply_remote_update(&self, item: &testtbItem) -> Result<String, String> {
        let existing = self.get_item_by_id(&item.id)?;
        
        match existing {
            None => {
                let sql = format!(
                    "INSERT INTO {} (id, cid, kind, item, data) VALUES (?, ?, ?, ?, ?)",
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
                    ],
                    &up,
                )?;
                self.logger.detail(&format!("远程数据插入: {}", item.id));
                Ok("inserted".to_string())
            }
            Some(_) => {
                let sql = format!(
                    "UPDATE {} SET cid = ?, kind = ?, item = ?, data = ? WHERE id = ?",
                    self.config.table_name
                );
                let up = UpInfo::new();
                self.db.do_m(
                    &sql,
                    &[
                        &item.cid as &dyn rusqlite::ToSql,
                        &item.kind,
                        &item.item,
                        &item.data,
                        &item.id,
                    ],
                    &up,
                )?;
                self.logger.detail(&format!("远程数据更新: {}", item.id));
                Ok("updated".to_string())
            }
        }
    }
}
