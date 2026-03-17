//! LoversDataState - 用户表数据状态机
//!
//! 职责：管理用户表和会话验证
//! 设计为单表状态机，包含 SID 验证功能
//!
//! 说明：此文件属于 axum78 业务层，不是 database 基类

use database::{DataSync, BaseState, TableConfig};
use serde::{Deserialize, Serialize};

/// 用户表创建SQL
pub const LOVERS_CREATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS lovers (
    idpk INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    uname TEXT NOT NULL DEFAULT '',
    idcodef TEXT NOT NULL DEFAULT '',
    upby TEXT NOT NULL DEFAULT '',
    cid TEXT NOT NULL DEFAULT '',
    uid TEXT NOT NULL DEFAULT '',
    uptime TEXT NOT NULL DEFAULT ''
)
"#;

/// 会话表创建SQL
pub const LOVERS_AUTH_CREATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS lovers_auth (
    idpk INTEGER PRIMARY KEY AUTOINCREMENT,
    ikuser INTEGER NOT NULL,
    sid TEXT NOT NULL,
    sid_web TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '',
    UNIQUE(sid),
    UNIQUE(sid_web)
)
"#;

/// SID 验证结果
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub cid: String,
    pub uid: String,
    pub uname: String,
}

impl VerifyResult {
    pub fn new(cid: &str, uid: &str, uname: &str) -> Self {
        Self {
            cid: cid.to_string(),
            uid: uid.to_string(),
            uname: uname.to_string(),
        }
    }
}

/// LoversDataState - 用户表数据状态机
#[derive(Clone, Serialize, Deserialize)]
pub struct LoversDataState {
    #[serde(flatten)]
    pub base: BaseState,

    #[serde(skip)]
    pub datasync: DataSync,
}

impl std::fmt::Debug for LoversDataState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoversDataState")
            .field("base", &self.base)
            .finish()
    }
}

impl LoversDataState {
    pub fn new() -> Self {
        Self {
            base: BaseState::new("lovers"),
            datasync: DataSync::new("lovers"),
        }
    }

    pub fn from_config(config: &TableConfig) -> Self {
        Self {
            base: BaseState::new(&config.name),
            datasync: DataSync::from_config(config),
        }
    }

    /// 初始化表
    pub fn init_tables(&self) -> Result<(), String> {
        let conn = self.datasync.db.get_conn();
        let conn_guard = conn.lock().map_err(|e| e.to_string())?;

        conn_guard
            .execute(LOVERS_CREATE_SQL, [])
            .map_err(|e| format!("创建用户表失败：{}", e))?;

        conn_guard
            .execute(LOVERS_AUTH_CREATE_SQL, [])
            .map_err(|e| format!("创建会话表失败：{}", e))?;

        Ok(())
    }

    /// 验证 SID（支持普通会话和Web会话）
    /// 
    /// 从数据库验证 SID 或 SID_WEB 是否有效
    /// 两端都可以登录，用 OR 查询
    pub fn verify_sid(&self, sid: &str) -> Result<VerifyResult, String> {
        if sid.is_empty() {
            return Err("无效的SID: sid为空".to_string());
        }

        let sql = r#"
            SELECT l.idpk, l.uname, l.idcodef as cid, l.id as uid 
            FROM lovers l 
            JOIN lovers_auth la ON l.idpk = la.ikuser 
            WHERE la.sid = ? OR la.sid_web = ?
        "#;

        let rows = self.datasync.do_get(sql, &[&sid as &dyn rusqlite::ToSql, &sid as &dyn rusqlite::ToSql])
            .map_err(|e| format!("验证失败: {}", e))?;

        if rows.is_empty() {
            return Err("无效的SID: 未找到会话".to_string());
        }

        let row = &rows[0];
        let cid = row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let uid = row.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let uname = row.get("uname").and_then(|v| v.as_str()).unwrap_or("").to_string();

        Ok(VerifyResult::new(&cid, &uid, &uname))
    }
}

impl Default for LoversDataState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_sid_empty() {
        let state = LoversDataState::new();
        let result = state.verify_sid("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("sid为空"));
    }

    #[test]
    fn test_verify_result_new() {
        let result = VerifyResult::new("CID001", "UID001", "Test User");
        assert_eq!(result.cid, "CID001");
        assert_eq!(result.uid, "UID001");
        assert_eq!(result.uname, "Test User");
    }
}
