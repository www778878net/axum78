//! BaseApi - 基础 CRUD API Trait
//!
//! 类似 koa78-base78 的 Base78 类，提供:
//! - get: 查询
//! - m_add: 新增 (m 前缀 = 修改操作)
//! - m_update: 更新
//! - m_del: 删除

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{UpInfo as ApiUpInfo, ApiError};
use database::Sqlite78;
use base::UpInfo;

/// 表配置
#[derive(Debug, Clone)]
pub struct TableConfig {
    /// 表名
    pub tbname: String,
    /// 主键字段 (默认 id)
    pub id_field: String,
    /// 自增主键 (默认 idpk)
    pub idpk_field: String,
    /// 用户隔离字段 (cid 或 uid)
    pub uidcid: String,
    /// 业务字段列表
    pub cols: Vec<String>,
}

/// BaseApi Trait - 所有表 API 的基础
#[async_trait]
pub trait BaseApi: Send + Sync + 'static {
    /// 实体类型
    type Entity: Serialize + for<'de> Deserialize<'de> + Send + Sync;

    /// 获取表配置
    fn config(&self) -> &TableConfig;

    /// 获取数据库连接
    fn db(&self) -> &Sqlite78;

    /// 获取上下文 (UpInfo)
    fn context(&self) -> &ApiUpInfo;

    /// 创建 UpInfo (用于数据库操作)
    fn create_up_info(&self) -> UpInfo {
        let ctx = self.context();
        UpInfo::new()
            .with_api(&ctx.apisys, &ctx.apimicro, &ctx.apiobj)
    }

    // ============ CRUD 方法 ============

    /// 查询列表
    async fn get(&self, params: HashMap<String, Value>) -> Result<Vec<Self::Entity>, ApiError> {
        let config = self.config();
        let up = self.create_up_info();

        // 构建 WHERE 条件
        let mut where_clause = format!("WHERE `{}` = ?", config.uidcid);
        let mut values: Vec<String> = vec![self.context().cid.clone()];

        for (key, val) in &params {
            if config.cols.contains(key) {
                where_clause.push_str(&format!(" AND `{}` = ?", key));
                values.push(val.as_str().unwrap_or("").to_string());
            }
        }

        let sql = format!(
            "SELECT * FROM {} {} ORDER BY idpk DESC LIMIT 100",
            config.tbname, where_clause
        );

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        self.db()
            .do_get(&sql, &params, &up)
            .map_err(|e| ApiError::new(&e, -3))?
            .into_iter()
            .map(|row| {
                serde_json::from_value(Value::Object(row.into_iter().collect()))
                    .map_err(|e| ApiError::new(&format!("解析失败: {}", e), -3))
            })
            .collect()
    }

    /// 根据 ID 查询单条
    async fn get_by_id(&self, id: &str) -> Result<Option<Self::Entity>, ApiError> {
        let config = self.config();
        let up = self.create_up_info();

        let sql = format!(
            "SELECT * FROM {} WHERE `{}` = ? AND `{}` = ? LIMIT 1",
            config.tbname, config.id_field, config.uidcid
        );

        let params: [&dyn rusqlite::ToSql; 2] = [&id, &self.context().cid];

        let rows = self.db()
            .do_get(&sql, &params, &up)
            .map_err(|e| ApiError::new(&e, -3))?;

        if rows.is_empty() {
            Ok(None)
        } else {
            let entity = serde_json::from_value(Value::Object(rows[0].clone().into_iter().collect()))
                .map_err(|e| ApiError::new(&format!("解析失败: {}", e), -3))?;
            Ok(Some(entity))
        }
    }

    /// 新增 (m 前缀 = 修改操作)
    async fn m_add(&self, entity: &Self::Entity) -> Result<String, ApiError> {
        let config = self.config();
        let ctx = self.context();
        let up = self.create_up_info();

        let json = serde_json::to_value(entity)
            .map_err(|e| ApiError::new(&format!("序列化失败: {}", e), -2))?;

        let obj = json.as_object().ok_or_else(|| ApiError::new("无效的实体格式", -2))?;

        // 构建插入 SQL
        let id = ApiUpInfo::new_id();
        let cols: Vec<String> = config.cols.iter()
            .filter(|c| obj.contains_key(*c))
            .cloned()
            .collect();

        let col_names: Vec<String> = cols.iter().map(|c| format!("`{}`", c)).collect();
        let placeholders: Vec<&str> = cols.iter().map(|_| "?").collect();

        let sql = format!(
            "INSERT INTO {} ({}, `{}`, `upby`, `uptime`, `{}`) VALUES ({}, ?, ?, ?, ?)",
            config.tbname,
            col_names.join(", "),
            config.id_field,
            config.uidcid,
            placeholders.join(", ")
        );

        // 构建参数
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for col in &cols {
            if let Some(val) = obj.get(col) {
                match val {
                    Value::String(s) => params.push(Box::new(s.clone())),
                    Value::Number(n) => params.push(Box::new(n.as_i64().unwrap_or(0))),
                    Value::Bool(b) => params.push(Box::new(*b as i64)),
                    _ => params.push(Box::new(val.to_string())),
                }
            }
        }
        params.push(Box::new(id.clone()));
        params.push(Box::new(ctx.uname.clone()));
        params.push(Box::new(ctx.uptime.clone()));
        params.push(Box::new(ctx.cid.clone()));

        let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        self.db()
            .do_m_add(&sql, &params_ref, &up)
            .map_err(|e| ApiError::new(&e, -3))?;

        Ok(id)
    }

    /// 更新 (m 前缀 = 修改操作)
    async fn m_update(&self, id: &str, entity: &Self::Entity) -> Result<bool, ApiError> {
        let config = self.config();
        let ctx = self.context();
        let up = self.create_up_info();

        let json = serde_json::to_value(entity)
            .map_err(|e| ApiError::new(&format!("序列化失败: {}", e), -2))?;

        let obj = json.as_object().ok_or_else(|| ApiError::new("无效的实体格式", -2))?;

        // 构建更新 SQL
        let cols: Vec<String> = config.cols.iter()
            .filter(|c| obj.contains_key(*c))
            .cloned()
            .collect();

        let set_clause: Vec<String> = cols.iter().map(|c| format!("`{}` = ?", c)).collect();

        let sql = format!(
            "UPDATE {} SET {}, `upby` = ?, `uptime` = ? WHERE `{}` = ? AND `{}` = ?",
            config.tbname,
            set_clause.join(", "),
            config.id_field,
            config.uidcid
        );

        // 构建参数
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for col in &cols {
            if let Some(val) = obj.get(col) {
                match val {
                    Value::String(s) => params.push(Box::new(s.clone())),
                    Value::Number(n) => params.push(Box::new(n.as_i64().unwrap_or(0))),
                    Value::Bool(b) => params.push(Box::new(*b as i64)),
                    _ => params.push(Box::new(val.to_string())),
                }
            }
        }
        params.push(Box::new(ctx.uname.clone()));
        params.push(Box::new(ctx.uptime.clone()));
        params.push(Box::new(id.to_string()));
        params.push(Box::new(ctx.cid.clone()));

        let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let result = self.db()
            .do_m(&sql, &params_ref, &up)
            .map_err(|e| ApiError::new(&e, -3))?;

        Ok(result.affected_rows > 0)
    }

    /// 删除 (m 前缀 = 修改操作)
    async fn m_del(&self, id: &str) -> Result<bool, ApiError> {
        let config = self.config();
        let up = self.create_up_info();

        let sql = format!(
            "DELETE FROM {} WHERE `{}` = ? AND `{}` = ?",
            config.tbname, config.id_field, config.uidcid
        );

        let params: [&dyn rusqlite::ToSql; 2] = [&id, &self.context().cid];

        let result = self.db()
            .do_m(&sql, &params, &up)
            .map_err(|e| ApiError::new(&e, -3))?;

        Ok(result.affected_rows > 0)
    }
}