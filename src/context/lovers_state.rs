//! LoversDataState - 用户表数据状态机
//!
//! 职责：管理用户表和会话验证
//! 设计为单表状态机，包含 SID 验证功能
//!
//! 说明：此文件属于 axum78 业务层，不是 database 基类
//!
//! ## MySQL 版本
//! LoversDataStateMysql - 基于 MySQL 的用户状态管理
//! 支持 find_or_create_user（企业微信登录）

use datastate::{DataSync, BaseState, TableConfig, Mysql78, MysqlConfig, MysqlUpInfo, next_id_string};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

/// 用户表创建SQL (SQLite)
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

/// 会话表创建SQL (SQLite)
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

/// 用户完整信息（用于登录返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户主键
    pub idpk: i64,
    /// 用户ID
    pub id: String,
    /// 用户名
    pub uname: String,
    /// 真实姓名
    pub truename: String,
    /// 公司ID
    pub idcodef: String,
    /// 公司名
    pub coname: String,
    /// 手机号
    pub mobile: String,
    /// 会话ID
    pub sid: String,
    /// 余额
    pub money78: i64,
    /// 消费
    pub consume: i64,
    /// 微信用户ID
    pub wechat_userid: String,
    /// 用户类型 (internal/external)
    pub user_type: String,
    /// 是否新用户
    pub is_new: bool,
}

impl UserInfo {
    /// 从数据库行创建（已存在用户）
    pub fn from_row(row: &HashMap<String, Value>, wechat_userid: &str, user_type: &str) -> Self {
        Self {
            idpk: row.get("idpk").and_then(|v| v.as_i64()).unwrap_or(0),
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            uname: row.get("uname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            truename: row.get("truename").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            idcodef: row.get("idcodef").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            coname: row.get("coname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            mobile: row.get("mobile").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            sid: row.get("sid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            money78: row.get("money78").and_then(|v| v.as_i64()).unwrap_or(0),
            consume: row.get("consume").and_then(|v| v.as_i64()).unwrap_or(0),
            wechat_userid: wechat_userid.to_string(),
            user_type: user_type.to_string(),
            is_new: false,
        }
    }

    /// 创建新用户信息
    pub fn new_user(idpk: i64, sid: &str, uname: &str, wechat_userid: &str, user_type: &str) -> Self {
        Self {
            idpk,
            id: sid.to_string(),
            uname: uname.to_string(),
            truename: wechat_userid.to_string(),
            idcodef: "GUEST000-8888-8888-8888-GUEST00GUEST".to_string(),
            coname: "测试帐套".to_string(),
            mobile: String::new(),
            sid: sid.to_string(),
            money78: 0,
            consume: 0,
            wechat_userid: wechat_userid.to_string(),
            user_type: user_type.to_string(),
            is_new: true,
        }
    }
}

/// LoversDataState - 用户表数据状态机 (SQLite 版本)
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
        let conn_guard = conn.blocking_lock();

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
    pub async fn verify_sid(&self, sid: &str) -> Result<VerifyResult, String> {
        if sid.is_empty() {
            return Err("无效的SID: sid为空".to_string());
        }

        let sql = r#"
            SELECT l.idpk, l.uname, l.idcodef as cid, l.id as uid 
            FROM lovers l 
            JOIN lovers_auth la ON l.idpk = la.ikuser 
            WHERE la.sid = ? OR la.sid_web = ?
        "#;

        let rows = self.datasync.do_get(sql, &[&sid as &dyn rusqlite::ToSql, &sid as &dyn rusqlite::ToSql]).await
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

// ============================================================
// MySQL 版本
// ============================================================

/// MySQL 连接池（全局单例）
static MYSQL_POOL: once_cell::sync::Lazy<Arc<Mutex<Option<Arc<Mysql78>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// 获取 MySQL 配置（从配置文件读取，优先使用环境变量）
fn get_mysql_config() -> MysqlConfig {
    let config = base::ProjectPath::find()
        .ok()
        .and_then(|p| p.load_ini_config().ok());

    let mysql_section = config.as_ref().and_then(|c| c.get("mysql"));

    MysqlConfig {
        host: std::env::var("MYSQL_HOST")
            .ok()
            .or_else(|| mysql_section.and_then(|s| s.get("host").cloned()))
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port: std::env::var("MYSQL_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .or_else(|| mysql_section.and_then(|s| s.get("port").and_then(|p| p.parse().ok())))
            .unwrap_or(3306),
        user: std::env::var("MYSQL_USER")
            .ok()
            .or_else(|| mysql_section.and_then(|s| s.get("user").cloned()))
            .unwrap_or_else(|| "root".to_string()),
        password: std::env::var("MYSQL_PASSWORD")
            .ok()
            .or_else(|| mysql_section.and_then(|s| s.get("password").cloned()))
            .unwrap_or_default(),
        database: std::env::var("MYSQL_DATABASE")
            .ok()
            .or_else(|| mysql_section.and_then(|s| s.get("database").cloned()))
            .unwrap_or_else(|| "testdb".to_string()),
        max_connections: mysql_section
            .and_then(|s| s.get("max_connections").and_then(|v| v.parse().ok()))
            .unwrap_or(10),
        is_log: mysql_section
            .and_then(|s| s.get("is_log").and_then(|v| v.parse().ok()))
            .unwrap_or(false),
        is_count: mysql_section
            .and_then(|s| s.get("is_count").and_then(|v| v.parse().ok()))
            .unwrap_or(false),
    }
}

/// 获取或初始化 MySQL 连接池
fn get_mysql_connection() -> Result<Arc<Mysql78>, String> {
    let pool = MYSQL_POOL.clone();
    let mut pool_guard = pool.lock().map_err(|e| format!("获取连接池锁失败: {}", e))?;
    
    if pool_guard.is_none() {
        let config = get_mysql_config();
        let mut mysql = Mysql78::new(config);
        mysql.initialize()?;
        *pool_guard = Some(Arc::new(mysql));
    }
    
    Ok(pool_guard.as_ref().unwrap().clone())
}

/// LoversDataStateMysql - 用户表数据状态机 (MySQL 版本)
/// 
/// 支持企业微信登录：find_or_create_user
#[derive(Clone)]
pub struct LoversDataStateMysql {
    /// MySQL 连接
    mysql: Arc<Mysql78>,
}

impl LoversDataStateMysql {
    /// 创建新实例
    pub fn new() -> Result<Self, String> {
        let mysql = get_mysql_connection()?;
        Ok(Self { mysql })
    }

    /// 使用现有连接创建
    pub fn with_mysql(mysql: Arc<Mysql78>) -> Self {
        Self { mysql }
    }

    /// 查找或创建用户（企业微信登录）
    ///
    /// # 参数
    /// - wechat_userid: 微信用户ID (FromUserName)
    /// - user_type: 用户类型 (internal/external)
    /// - corp_id: 企业ID
    ///
    /// # 返回
    /// - UserInfo: 用户完整信息
    ///
    /// # 表结构说明
    /// - lovers: id (主键), uname, idcodef (公司ID), cid, truename, mobile
    /// - lovers_auth: id, ikuser (关联 lovers.id), sid, sid_web
    pub fn find_or_create_user(
        &self,
        wechat_userid: &str,
        user_type: &str,
        corp_id: &str,
    ) -> Result<UserInfo, String> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let sid = next_id_string();

        // 使用 corp_id + user_type + wechatUserId 组合作为全局唯一的用户名
        let uname = format!("{}_{}_{}", corp_id, user_type, wechat_userid);

        let up = MysqlUpInfo::new();

        // 查询用户是否已存在
        let query = "SELECT * FROM lovers WHERE uname = ?";
        let rows = self.mysql.do_get(query, vec![Value::String(uname.clone())], &up)
            .map_err(|e| format!("查询用户失败: {}", e))?;

        if !rows.is_empty() {
            // 用户已存在，更新 SID
            let user = &rows[0];

            // 使用 idpk 字段（数字类型）作为关联键
            let user_id = user.get("idpk")
                .and_then(|v| v.as_i64().map(|n| n.to_string()))
                .or_else(|| user.get("idpk").and_then(|v| v.as_u64().map(|n| n.to_string())))
                .or_else(|| user.get("id").and_then(|v| v.as_str().map(|s| s.to_string())))
                .or_else(|| user.get("id").and_then(|v| v.as_i64().map(|n| n.to_string())))
                .or_else(|| user.get("id").and_then(|v| v.as_u64().map(|n| n.to_string())))
                .unwrap_or_default();

            tracing::info!("用户已存在: user_id={}, uname={}", user_id, uname);

            if user_id.is_empty() {
                return Err("用户 ID 为空".to_string());
            }

            // 更新 lovers_auth 表的 sid (使用 ikuser 关联)
            let update_sid = "UPDATE lovers_auth SET sid = ?, uptime = ? WHERE ikuser = ?";
            self.mysql.do_m(update_sid, vec![
                Value::String(sid.clone()),
                Value::String(now.clone()),
                Value::String(user_id.to_string()),
            ], &up).map_err(|e| format!("更新SID失败: {}", e))?;

            // 获取用户完整信息
            let user_query = r#"
                SELECT l.id, l.idpk, l.uname, l.idcodef, l.cid, l.truename, l.mobile, la.sid
                FROM lovers l
                JOIN lovers_auth la ON l.idpk = la.ikuser
                WHERE l.idpk = ?
            "#;

            let user_rows = self.mysql.do_get(user_query, vec![Value::String(user_id.to_string())], &up)
                .map_err(|e| format!("获取用户信息失败: {}", e))?;

            if !user_rows.is_empty() {
                let row = &user_rows[0];
                // 使用 idpk 作为主键
                let idpk = row.get("idpk")
                    .and_then(|v| v.as_i64())
                    .or_else(|| row.get("idpk").and_then(|v| v.as_u64().map(|n| n as i64)))
                    .unwrap_or(0);
                
                let user_id_from_row = row.get("id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| idpk.to_string());

                return Ok(UserInfo {
                    idpk,
                    id: user_id_from_row,
                    uname: row.get("uname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    truename: row.get("truename").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    idcodef: row.get("idcodef").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    coname: String::new(),
                    mobile: row.get("mobile").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    sid: row.get("sid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    money78: 0,
                    consume: 0,
                    wechat_userid: wechat_userid.to_string(),
                    user_type: user_type.to_string(),
                    is_new: false,
                });
            } else {
                // 没有 auth 记录，检查是否已存在 ikuser 记录
                let check_auth = "SELECT * FROM lovers_auth WHERE ikuser = ?";
                let auth_rows = self.mysql.do_get(check_auth, vec![Value::String(user_id.to_string())], &up)
                    .map_err(|e| format!("检查认证记录失败: {}", e))?;

                if !auth_rows.is_empty() {
                    // 已存在认证记录，更新 SID
                    let update_sid = "UPDATE lovers_auth SET sid = ?, uptime = ? WHERE ikuser = ?";
                    self.mysql.do_m(update_sid, vec![
                        Value::String(sid.clone()),
                        Value::String(now.clone()),
                        Value::String(user_id.to_string()),
                    ], &up).map_err(|e| format!("更新SID失败: {}", e))?;

                    return Ok(UserInfo {
                        idpk: 0,
                        id: user_id.to_string(),
                        uname: uname.clone(),
                        truename: wechat_userid.to_string(),
                        idcodef: user.get("idcodef").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        coname: String::new(),
                        mobile: String::new(),
                        sid,
                        money78: 0,
                        consume: 0,
                        wechat_userid: wechat_userid.to_string(),
                        user_type: user_type.to_string(),
                        is_new: false,
                    });
                } else {
                    // 创建新的认证记录
                    let auth_id = next_id_string();
                    let auth_insert = r#"
                        INSERT INTO lovers_auth (id, ikuser, sid, sid_web, sid_web_date, upby, uptime, uid, pwd)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#;
                    self.mysql.do_m_add(auth_insert, vec![
                        Value::String(auth_id),
                        Value::String(user_id.to_string()),
                        Value::String(sid.clone()),
                        Value::String(sid.clone()),
                        Value::String(now.clone()),
                        Value::String(uname.clone()),
                        Value::String(now.clone()),
                        Value::String(sid.clone()),
                        Value::String(String::new()),
                    ], &up).map_err(|e| format!("创建认证记录失败: {}", e))?;

                    return Ok(UserInfo {
                        idpk: 0,
                        id: user_id.to_string(),
                        uname: uname.clone(),
                        truename: wechat_userid.to_string(),
                        idcodef: user.get("idcodef").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        coname: String::new(),
                        mobile: String::new(),
                        sid,
                        money78: 0,
                        consume: 0,
                        wechat_userid: wechat_userid.to_string(),
                        user_type: user_type.to_string(),
                        is_new: false,
                    });
                }
            }
        }

        // 用户不存在，创建新用户
        let cid_guest = "GUEST000-8888-8888-8888-GUEST00GUEST";
        let user_id = next_id_string();

        // 插入 lovers 表
        let lovers_insert = r#"
            INSERT INTO lovers (id, uname, truename, idcodef, cid, referrer, mobile, openweixin, upby, uptime)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;
        let lovers_values = vec![
            Value::String(user_id.clone()),
            Value::String(uname.clone()),
            Value::String(wechat_userid.to_string()),
            Value::String(cid_guest.to_string()),
            Value::String(cid_guest.to_string()),
            Value::String(String::new()),
            Value::String(String::new()),
            Value::String("企业微信".to_string()),
            Value::String(uname.clone()),
            Value::String(now.clone()),
        ];

        // 执行插入并获取自动生成的 idpk
        let insert_result = self.mysql.do_m_add(lovers_insert, lovers_values, &up)
            .map_err(|e| format!("创建用户失败: {}", e))?;
        
        // 获取新插入用户的 idpk
        let idpk = if insert_result.insert_id > 0 {
            insert_result.insert_id as i64
        } else {
            // 如果无法获取 insert_id，查询获取
            let query = "SELECT idpk FROM lovers WHERE id = ?";
            let rows = self.mysql.do_get(query, vec![Value::String(user_id.clone())], &up)
                .map_err(|e| format!("获取用户 idpk 失败: {}", e))?;
            
            if !rows.is_empty() {
                rows[0].get("idpk").and_then(|v| v.as_i64())
                    .or_else(|| rows[0].get("idpk").and_then(|v| v.as_u64().map(|n| n as i64)))
                    .unwrap_or(0)
            } else {
                0
            }
        };

        // 插入 lovers_auth 表
        let auth_id = next_id_string();
        let auth_insert = r#"
            INSERT INTO lovers_auth (id, ikuser, sid, sid_web, sid_web_date, upby, uptime, uid, pwd)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;
        let auth_values = vec![
            Value::String(auth_id),
            Value::String(idpk.to_string()),
            Value::String(sid.clone()),
            Value::String(sid.clone()),
            Value::String(now.clone()),
            Value::String(uname.clone()),
            Value::String(now.clone()),
            Value::String(sid.clone()),
            Value::String(String::new()),
        ];
        self.mysql.do_m_add(auth_insert, auth_values, &up)
            .map_err(|e| format!("创建认证记录失败: {}", e))?;

        tracing::info!("创建新用户: user_id={}, idpk={}, uname={}", user_id, idpk, uname);

        Ok(UserInfo {
            idpk,
            id: user_id,
            uname,
            truename: wechat_userid.to_string(),
            idcodef: cid_guest.to_string(),
            coname: String::new(),
            mobile: String::new(),
            sid,
            money78: 0,
            consume: 0,
            wechat_userid: wechat_userid.to_string(),
            user_type: user_type.to_string(),
            is_new: true,
        })
    }

    /// 验证 SID
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

        let up = MysqlUpInfo::new();
        let rows = self.mysql.do_get(sql, vec![
            Value::String(sid.to_string()),
            Value::String(sid.to_string()),
        ], &up).map_err(|e| format!("验证失败: {}", e))?;

        if rows.is_empty() {
            return Err("无效的SID: 未找到会话".to_string());
        }

        let row = &rows[0];
        let cid = row.get("cid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let uid = row.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let uname = row.get("uname").and_then(|v| v.as_str()).unwrap_or("").to_string();

        Ok(VerifyResult::new(&cid, &uid, &uname))
    }

    /// 根据 SID 获取用户完整信息
    pub fn get_user_by_sid(&self, sid: &str) -> Result<UserInfo, String> {
        if sid.is_empty() {
            return Err("无效的SID: sid为空".to_string());
        }

        let sql = r#"
            SELECT l.*, la.sid, lb.money78, lb.consume, c.coname
            FROM lovers l
            JOIN lovers_auth la ON l.idpk = la.ikuser
            JOIN lovers_balance lb ON l.idpk = lb.ikuser
            LEFT JOIN companys c ON l.idcodef = c.id
            WHERE la.sid = ? OR la.sid_web = ?
        "#;

        let up = MysqlUpInfo::new();
        let rows = self.mysql.do_get(sql, vec![
            Value::String(sid.to_string()),
            Value::String(sid.to_string()),
        ], &up).map_err(|e| format!("查询用户失败: {}", e))?;

        if rows.is_empty() {
            return Err("用户不存在".to_string());
        }

        let row = &rows[0];
        
        // 从 uname 解析 user_type
        let uname = row.get("uname").and_then(|v| v.as_str()).unwrap_or("");
        let parts: Vec<&str> = uname.split('_').collect();
        let user_type = parts.get(1).unwrap_or(&"internal").to_string();
        let wechat_userid = parts.get(2).unwrap_or(&"").to_string();

        Ok(UserInfo::from_row(row, &wechat_userid, &user_type))
    }
}

impl Default for LoversDataStateMysql {
    fn default() -> Self {
        Self::new().expect("Failed to create LoversDataStateMysql")
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

    #[test]
    fn test_user_info_new_user() {
        let user = UserInfo::new_user(1, "sid123", "corp_internal_wx001", "wx001", "internal");
        assert_eq!(user.idpk, 1);
        assert_eq!(user.sid, "sid123");
        assert_eq!(user.wechat_userid, "wx001");
        assert_eq!(user.user_type, "internal");
        assert!(user.is_new);
    }
}
