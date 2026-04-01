//! Base78 - 控制器基类
//!
//! 参考TypeScript版本的Base78，提供：
//! - 组合DataState（数据操作 + 审计）
//! - 处理UpInfo（请求参数）
//! - 返回格式化（Response）
//! - 权限检查
//! - 通用CRUD方法

use datastate::{DataState, LocalDB};
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
    pub fn check_admin_permission(&self, up: &UpInfo) -> Result<(), String> {
        if self.isadmin {
            // TODO: 实现管理员权限检查
            // 需要从配置中读取cidvps和cidmy
            // if up.cid != config.get('cidvps') && up.cid != config.get('cidmy') && !up.uname.contains("sys") {
            //     return Err("只有管理员可以操作".to_string());
            // }
        }
        Ok(())
    }

    /// 验证参数数量
    pub fn validate_params(&self, up: &UpInfo, required_count: usize) -> Result<Vec<String>, String> {
        // 从jsdata中解析参数
        let pars_data: Vec<String> = if let Some(jsdata_str) = &up.jsdata {
            if let Ok(data) = serde_json::from_str::<Vec<String>>(jsdata_str) {
                data
            } else {
                return Err("参数格式错误".to_string());
            }
        } else {
            return Err("缺少参数".to_string());
        };
        
        if pars_data.len() < required_count {
            return Err(format!("参数数量不足，需要{}个参数，实际{}个", required_count, pars_data.len()));
        }
        
        Ok(pars_data)
    }

    /// 验证必填字段
    pub fn validate_required(&self, value: &str, field_name: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err(format!("{}不能为空", field_name));
        }
        Ok(())
    }

    /// 验证数字范围
    pub fn validate_range(&self, value: i32, min: i32, max: i32, field_name: &str) -> Result<(), String> {
        if value < min || value > max {
            return Err(format!("{}必须在{}到{}之间", field_name, min, max));
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
    pub async fn do_get(&self, sql: &str, params: Vec<String>) -> Result<Vec<HashMap<String, Value>>, String> {
        self.logger.detail(&format!("执行SQL: {}", sql));
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        self.datastate.do_get(sql, &params_refs, "base78", "do_get")
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

    pub async fn do_get(&self, sql: &str, params: Vec<String>) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.do_get(sql, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base78_new() {
        let base = Base78::new("test_table", "cid");
        assert_eq!(base.tbname, "test_table");
        assert_eq!(base.uidcid, "cid");
        assert!(!base.isadmin);
    }

    #[test]
    fn test_base78_set_admin() {
        let mut base = Base78::new("test_table", "cid");
        base.set_admin();
        assert!(base.isadmin);
    }

    #[test]
    fn test_cid_base78_new() {
        let cid_base = CidBase78::new("test_table");
        assert_eq!(cid_base.base.tbname, "test_table");
        assert_eq!(cid_base.base.uidcid, "cid");
    }

    #[test]
    fn test_cid_base78_set_admin() {
        let mut cid_base = CidBase78::new("test_table");
        cid_base.set_admin();
        assert!(cid_base.base.isadmin);
    }

    #[test]
    fn test_validate_params_success() {
        let base = Base78::new("test_table", "cid");
        let mut up = UpInfo::new();
        up.jsdata = Some(r#"["param1", "param2", "param3"]"#.to_string());

        let result = base.validate_params(&up, 2);
        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_validate_params_insufficient() {
        let base = Base78::new("test_table", "cid");
        let mut up = UpInfo::new();
        up.jsdata = Some(r#"["param1"]"#.to_string());

        let result = base.validate_params(&up, 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("参数数量不足"));
    }

    #[test]
    fn test_validate_params_missing_jsdata() {
        let base = Base78::new("test_table", "cid");
        let up = UpInfo::new();

        let result = base.validate_params(&up, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("缺少参数"));
    }

    #[test]
    fn test_validate_params_invalid_json() {
        let base = Base78::new("test_table", "cid");
        let mut up = UpInfo::new();
        up.jsdata = Some("invalid json".to_string());

        let result = base.validate_params(&up, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("参数格式错误"));
    }

    #[test]
    fn test_validate_required_empty() {
        let base = Base78::new("test_table", "cid");
        let result = base.validate_required("", "字段名");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不能为空"));
    }

    #[test]
    fn test_validate_required_not_empty() {
        let base = Base78::new("test_table", "cid");
        let result = base.validate_required("value", "字段名");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_range_valid() {
        let base = Base78::new("test_table", "cid");
        let result = base.validate_range(50, 0, 100, "数值");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_range_invalid_too_small() {
        let base = Base78::new("test_table", "cid");
        let result = base.validate_range(-1, 0, 100, "数值");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("必须在"));
    }

    #[test]
    fn test_validate_range_invalid_too_large() {
        let base = Base78::new("test_table", "cid");
        let result = base.validate_range(101, 0, 100, "数值");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("必须在"));
    }

    #[test]
    fn test_check_admin_permission_not_admin() {
        let base = Base78::new("test_table", "cid");
        let up = UpInfo::new();
        // isadmin 为 false，应该直接返回 Ok
        let result = base.check_admin_permission(&up);
        assert!(result.is_ok());
    }
}
