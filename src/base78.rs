//! Base78 - 控制器基类
//!
//! 参考TypeScript版本的Base78，提供：
//! - 组合DataState（数据操作 + 审计）
//! - 处理UpInfo（请求参数）
//! - 返回格式化（Response）
//! - 权限检查
//! - 通用CRUD方法

use database::{DataState, LocalDB};
use base::{MyLogger, UpInfo};
use serde_json::Value;
use std::collections::HashMap;

/// Base78 - 控制器基类
pub struct Base78 {
    /// 表名
    pub tbname: String,
    /// 隔离字段（cid或uid）
    pub uidcid: String,
    /// 数据状态（数据操作 + 审计）
    pub datastate: DataState,
    /// 日志
    pub logger: MyLogger,
    /// 是否为管理员表
    pub isadmin: bool,
}

impl Base78 {
    /// 创建Base78实例
    pub fn new(tbname: &str, uidcid: &str) -> Self {
        Self {
            tbname: tbname.to_string(),
            uidcid: uidcid.to_string(),
            datastate: DataState::with_db(tbname, LocalDB::new(None).unwrap()),
            logger: MyLogger::new(tbname, 7),
            isadmin: false,
        }
    }

    /// 设置为管理员表
    pub fn set_admin(&mut self) {
        self.isadmin = true;
    }

    /// 检查管理员权限
    pub fn check_admin_permission(&self, _up: &UpInfo) -> Result<(), String> {
        if self.isadmin {
            // TODO: 实现管理员权限检查
        }
        Ok(())
    }

    /// 查询记录（支持colp参数）
    /// - colp: WHERE条件字段名
    /// - 数据从up.jsdata中解析
    pub async fn get(&self, up: &UpInfo, colp: Option<&[&str]>) -> Result<Vec<HashMap<String, Value>>, String> {
        let colp = colp.unwrap_or(&[]);
        
        // 从jsdata中解析参数
        let pars_data: Vec<String> = if let Some(jsdata_str) = &up.jsdata {
            if let Ok(data) = serde_json::from_str::<Vec<String>>(jsdata_str) {
                data
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        
        let mut where_clause = format!("{} = ?", self.uidcid);
        let mut params: Vec<String> = vec![up.cid.clone()];
        
        for (i, col) in colp.iter().enumerate() {
            if i < pars_data.len() {
                where_clause.push_str(&format!(" AND {} = ?", col));
                params.push(pars_data[i].clone());
            }
        }
        
        let sql = format!(
            "SELECT * FROM {} WHERE {} ORDER BY {} LIMIT {}, {}",
            self.tbname, where_clause, up.order, up.getstart, up.getnumber
        );
        
        self.logger.detail(&format!("执行SQL: {}", sql));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        self.datastate.do_get(&sql, &params_refs, "base78", "get")
    }

    /// 查询所有记录
    pub async fn get_all(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        let sql = format!(
            "SELECT * FROM {} WHERE {} = ? ORDER BY {} LIMIT {}, {}",
            self.tbname, self.uidcid, up.order, up.getstart, up.getnumber
        );
        
        let params: Vec<&dyn rusqlite::ToSql> = vec![&up.cid];
        
        self.logger.detail(&format!("执行SQL: {}", sql));
        
        self.datastate.do_get(&sql, &params, "base78", "get_all")
    }

    /// 根据ID查询
    pub async fn get_by_id(&self, up: &UpInfo, id: &str) -> Result<Option<HashMap<String, Value>>, String> {
        self.datastate.get_one(id, "base78", "get_by_id")
    }

    /// 添加记录
    pub async fn m_add(&self, up: &UpInfo, record: &HashMap<String, Value>) -> Result<String, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_add(record, "base78", "m_add")
    }

    /// 更新记录
    pub async fn m_update(&self, up: &UpInfo, id: &str, record: &HashMap<String, Value>) -> Result<bool, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_update(id, record, "base78", "m_update")
    }

    /// 删除记录
    pub async fn m_del(&self, up: &UpInfo, id: &str) -> Result<bool, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_del(id, "base78", "m_del")
    }

    /// 执行自定义查询
    pub async fn do_get(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<HashMap<String, Value>>, String> {
        self.logger.detail(&format!("执行SQL: {}", sql));
        self.datastate.do_get(sql, params, "base78", "do_get")
    }
}

/// CidBase78 - 基于CID隔离的控制器基类
pub struct CidBase78 {
    pub base: Base78,
}

impl CidBase78 {
    pub fn new(tbname: &str) -> Self {
        Self {
            base: Base78::new(tbname, "cid"),
        }
    }

    pub fn set_admin(&mut self) {
        self.base.set_admin();
    }

    pub async fn get_all(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.get_all(up).await
    }

    pub async fn get_by_id(&self, up: &UpInfo, id: &str) -> Result<Option<HashMap<String, Value>>, String> {
        self.base.get_by_id(up, id).await
    }

    pub async fn m_add(&self, up: &UpInfo, record: &HashMap<String, Value>) -> Result<String, String> {
        self.base.m_add(up, record).await
    }

    pub async fn m_update(&self, up: &UpInfo, id: &str, record: &HashMap<String, Value>) -> Result<bool, String> {
        self.base.m_update(up, id, record).await
    }

    pub async fn m_del(&self, up: &UpInfo, id: &str) -> Result<bool, String> {
        self.base.m_del(up, id).await
    }

    pub async fn do_get(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.do_get(sql, params).await
    }
}
