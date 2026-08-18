//! Base78 - 控制器基类
//!
//! 参考TypeScript版本的Base78，提供：
//! - 组合DataState（数据操作 + 审计）
//! - 处理UpInfo（请求参数）
//! - 返回格式化（Response）
//! - 权限检查
//! - 通用CRUD方法

use axum::{
    body::Bytes,
    http::{Method, StatusCode},
};
use datastate::{DataState, LocalDB, Mysql78, MysqlUpInfo};
use crate::base::{MyLogger, Response, UpInfo};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::{Controller78, async_trait};

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
        
        let params_values: Vec<rusqlite::types::Value> = params.iter().map(|p| rusqlite::types::Value::Text(p.clone())).collect();
        self.datastate.do_get(&sql, params_values, "base78", "get").await
    }

    /// 查询所有记录
    pub async fn get_all(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        let sql = format!(
            "SELECT * FROM {} WHERE {} = ? ORDER BY {} LIMIT {}, {}",
            self.tbname, self.uidcid, up.order, up.getstart, up.getnumber
        );
        
        self.logger.detail(&format!("执行SQL: {}", sql));
        
        self.datastate.do_get(&sql, vec![rusqlite::types::Value::Text(up.cid.clone())], "base78", "get_all").await
    }

    /// 根据ID查询
    pub async fn get_by_id(&self, up: &UpInfo, id: &str) -> Result<Option<HashMap<String, Value>>, String> {
        self.datastate.get_one(id, "base78", "get_by_id").await
    }

    /// 添加记录
    pub async fn m_add(&self, up: &UpInfo, record: &HashMap<String, Value>) -> Result<String, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_add(record, "base78", "m_add").await
    }

    /// 更新记录
    pub async fn m_update(&self, up: &UpInfo, id: &str, record: &HashMap<String, Value>) -> Result<bool, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_update(id, record, "base78", "m_update").await
    }

    /// 删除记录
    pub async fn m_del(&self, up: &UpInfo, id: &str) -> Result<bool, String> {
        self.check_admin_permission(up)?;
        self.datastate.m_del(id, "base78", "m_del").await
    }

    /// 执行自定义查询
    pub async fn do_get(&self, sql: &str, params: Vec<String>) -> Result<Vec<HashMap<String, Value>>, String> {
        self.logger.detail(&format!("执行SQL: {}", sql));
        let params_values: Vec<rusqlite::types::Value> = params.iter().map(|p| rusqlite::types::Value::Text(p.clone())).collect();
        self.datastate.do_get(sql, params_values, "base78", "do_get").await
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

// ============ MySQL 版 Base78 ============

/// MysqlBase78 - MySQL 版控制器基类
///
/// 对应 NodeJS Base78.ts，内置 get 方法自动拼 SQL，
/// 子类只需指定表名，不需要手写 SQL。
///
/// 用法：
///   let ctrl = MysqlBase78::new("steam_item_store", "cid", mysql, vec!["name", "phone"]);
///   let rows = ctrl.get(&up).await?;
///
/// 列名白名单（cols_imp）说明：
/// - 写操作（madd/mupdate/mupdatebyid/m/mbyid）时，jsdata 里的每个 KEY 必须命中白名单，否则拒绝，防止 SQL 注入。
/// - 白名单为空（vec![]）表示该表只读：写操作直接报错「未配置列白名单」，读操作不受影响。
/// - 系统列（id/upby/uptime/uidcid）由框架自动追加，业务 JSON 无需也不允许传入。
pub struct MysqlBase78 {
    pub tbname: String,
    pub uidcid: String,
    pub mysql: Arc<Mysql78>,
    /// 业务列名白名单（防止 SQL 注入）
    pub cols_imp: Vec<String>,
    /// 白名单哈希集合（构造时建立，O(1) 查找，大小写不敏感）
    cols_imp_set: HashSet<String>,
    /// 账套公开查询开关（默认关闭，防止越权）
    pub allow_bcid: bool,
}

/// 系统列：由框架自动追加，业务 JSON 不允许传入
const SYSTEM_COLS: [&str; 4] = ["id", "upby", "uptime", "uidcid"];

impl MysqlBase78 {
    pub fn new(tbname: &str, uidcid: &str, mysql: Arc<Mysql78>, cols_imp: Vec<String>) -> Self {
        let cols_imp_set: HashSet<String> = cols_imp.iter().map(|c| c.to_lowercase()).collect();
        Self {
            tbname: tbname.to_string(),
            uidcid: uidcid.to_string(),
            mysql,
            cols_imp,
            cols_imp_set,
            allow_bcid: false,
        }
    }

    /// 通用只读查询（对应 NodeJS Base78.get，第759行）
    ///
    /// 条件列取「已验证」的 `up.wherecols`（由 `check_request` 从 `wherecolsn` 校验写入），
    /// 值从 `up.jsdata` 解析的同名列取（对齐 `m_add`/`m_update` 的 `parse_jsdata_kv`）。
    /// 列名经 `cols_imp` 白名单校验防注入（在 `check_request` 内完成）。
    /// SELECT 固定 `*`（对齐 TS）。
    ///
    /// SQL: SELECT * FROM `{tbname}` WHERE `{uidcid}`=? [AND col=? ...]
    ///      ORDER BY {validated_order} LIMIT {up.getnumber} OFFSET {up.getstart}
    pub async fn get(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        // 先控制器（注册时已确定列），再校验：用「控制器已知的列」验证请求里的 wherecolsn / order
        let mut up = up.clone();
        self.check_request(&mut up)?;
        let order = self.validated_order(&up.order);

        // 条件列：已验证的 wherecols；值从 jsdata 同名列取
        let where_values = self.where_values_from_jsdata(up)?;
        let (where_clause, params) = self.build_where(&up.wherecols, &where_values, &up.cid)?;

        let sql = format!(
            "SELECT * FROM `{}` WHERE {} ORDER BY {} LIMIT {} OFFSET {}",
            self.tbname, where_clause, order, up.getnumber, up.getstart
        );

        let up_info = datastate::MysqlUpInfo::new();

        self.mysql.do_get(&sql, params, &up_info)
            .map_err(|e| format!("查询失败: {}", e))
    }

    /// 自定义查询
    pub async fn do_get(&self, sql: &str, params: Vec<Value>) -> Result<Vec<HashMap<String, Value>>, String> {
        let up_info = datastate::MysqlUpInfo::new();
        self.mysql.do_get(sql, params, &up_info)
            .map_err(|e| format!("查询失败: {}", e))
    }

    // ============ 扩展开关 ============

    /// 开启账套公开查询（getby_bcid），默认关闭（越权保护）
    pub fn set_allow_bcid(&mut self) {
        self.allow_bcid = true;
    }

    /// 账套公开查询（对应 NodeJS Base78.getbyBcid，第718行）
    ///
    /// `WHERE cid = ?`（值 `up.bcid`），可读整个账套数据，属越权面，
    /// 必须显式通过 `set_allow_bcid` 开启，否则拒绝（默认关闭）。
    /// 条件列取「已验证」的 `up.wherecols`，值从 `up.jsdata` 解析的同名列取。
    pub async fn getby_bcid(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        if !self.allow_bcid {
            return Err("getbyBcid not allowed for this table".to_string());
        }
        if up.bcid.is_empty() {
            return Err("缺少 bcid 参数".to_string());
        }

        // 先控制器（注册时已确定列），再校验：用「控制器已知的列」验证请求里的 wherecolsn / order
        let mut up = up.clone();
        self.check_request(&mut up)?;
        let order = self.validated_order(&up.order);
        let where_values = self.where_values_from_jsdata(up)?;
        let (where_clause, params) = self.build_where_bcid(&up.wherecols, &where_values, &up.bcid)?;

        let sql = format!(
            "SELECT * FROM `{}` WHERE {} ORDER BY {} LIMIT {} OFFSET {}",
            self.tbname, where_clause, order, up.getnumber, up.getstart
        );

        let up_info = datastate::MysqlUpInfo::new();

        self.mysql.do_get(&sql, params, &up_info)
            .map_err(|e| format!("查询失败: {}", e))
    }

    // ============ CRUD 写方法（参考 koabase78 Base78.ts） ============
    //
    // 参数约定：统一从 `up.jsdata` 解析，jsdata 就是「一行数据的 KV 对象」：
    //   {"col1":"val1","col2":"val2",...}
    //
    // 安全约定（对齐 koabase78「验证 KEY 防注入」）：
    //   - 列名（KEY）：必须命中注册时注入的白名单 cols_imp，否则整体拒绝。
    //   - 值（VALUE）：全部走 `?` 占位符参数化（Mysql78 底层支持）。
    //   - 系统字段：id（雪花）、upby、uptime、uidcid 由框架自动追加，业务 JSON 不允许传入。

    /// 系统列集合（含 uidcid 隔离列），用于排除业务 JSON 传入的系统字段
    fn is_system_col(&self, col: &str) -> bool {
        SYSTEM_COLS.contains(&col.to_lowercase().as_str()) || col.eq_ignore_ascii_case(&self.uidcid)
    }

    /// 从 jsdata 解析「一行数据 KV 对象」，返回 (cols, vals) 有序对
    ///
    /// jsdata 必须是 JSON 对象：{"col1":"val1","col2":"val2",...}
    fn parse_jsdata_kv(up: &UpInfo) -> Result<(Vec<String>, Vec<Value>), String> {
        let jsdata = up.jsdata.as_deref().ok_or("缺少 jsdata 参数")?;
        let value: Value = serde_json::from_str(jsdata).map_err(|e| format!("jsdata 解析失败: {}", e))?;

        let map = match value {
            Value::Object(map) => map,
            _ => return Err("jsdata 必须是 KV 对象，如 {\"col1\":\"val1\"}".to_string()),
        };

        let mut cols = Vec::with_capacity(map.len());
        let mut vals = Vec::with_capacity(map.len());
        for (k, v) in map {
            cols.push(k);
            vals.push(v);
        }
        Ok((cols, vals))
    }

    /// 校验列名白名单（防 SQL 注入），返回校验通过的 (cols, vals)
    ///
    /// - 白名单为空：直接报错「未配置列白名单」，禁止写操作。
    /// - KEY 不在白名单：报错并指明非法列名。
    /// - 系统列（id/upby/uptime/uidcid）由框架自动追加，业务 JSON 传入则拒绝。
    fn validate_cols(&self, cols: &[String], vals: &[Value]) -> Result<Vec<(String, Value)>, String> {
        if self.cols_imp_set.is_empty() {
            return Err(format!("表 `{}` 未配置列白名单，禁止写操作", self.tbname));
        }

        let mut result = Vec::with_capacity(cols.len());
        for (i, col) in cols.iter().enumerate() {
            if self.is_system_col(col) {
                return Err(format!("非法列名 `{}`（系统字段由框架自动填充，禁止业务传入）", col));
            }
            let lower = col.to_lowercase();
            if !self.cols_imp_set.contains(&lower) {
                return Err(format!("非法列名 `{}`（不在表 `{}` 的列白名单内）", col, self.tbname));
            }
            result.push((col.clone(), vals[i].clone()));
        }
        Ok(result)
    }

    /// 构建 WHERE 子句与参数（防 SQL 注入）
    ///
    /// 列名来自「已验证」的 `wherecols`，值来自 `up.jsdata` 解析的同名列，
    /// 逐列经 `cols_imp` 白名单校验，非法即整体报错。`pars` 不足时截断对齐（对齐 TS）。
    ///
    /// - 起点固定 `WHERE {uidcid}=?`，参数首元素为 `cid`。
    /// - 返回 `(where_clause, params)`，params 已按占位符顺序排列。
    fn build_where(&self, cols: &[String], pars: &[Value], cid: &str) -> Result<(String, Vec<Value>), String> {
        let mut where_clause = format!("`{}` = ?", self.uidcid);
        let mut params: Vec<Value> = vec![Value::String(cid.to_string())];

        let count = cols.len().min(pars.len());
        for i in 0..count {
            let col = &cols[i];
            let lower = col.to_lowercase();
            if !self.is_system_col(col) && !self.cols_imp_set.contains(&lower) {
                return Err(format!("非法列名 `{}`（不在表 `{}` 的列白名单内）", col, self.tbname));
            }
            where_clause.push_str(&format!(" AND `{}` = ?", col));
            params.push(pars[i].clone());
        }

        Ok((where_clause, params))
    }

    /// 构建账套公开查询的 WHERE 子句（getby_bcid 专用）
    ///
    /// 列名来自「已验证」的 `up.wherecols`，值来自 `up.jsdata` 解析的同名列，逐列经白名单校验（防注入）。
    /// 起点固定 `WHERE cid = ?`，参数首元素为 `up.bcid`。
    fn build_where_bcid(&self, cols: &[String], pars: &[Value], bcid: &str) -> Result<(String, Vec<Value>), String> {
        let mut where_clause = "`cid` = ?".to_string();
        let mut params: Vec<Value> = vec![Value::String(bcid.to_string())];

        let count = cols.len().min(pars.len());
        for i in 0..count {
            let col = &cols[i];
            let lower = col.to_lowercase();
            if !self.is_system_col(col) && !self.cols_imp_set.contains(&lower) {
                return Err(format!("非法列名 `{}`（不在表 `{}` 的列白名单内）", col, self.tbname));
            }
            where_clause.push_str(&format!(" AND `{}` = ?", col));
            params.push(pars[i].clone());
        }

        Ok((where_clause, params))
    }

    /// 从 `up.jsdata` 解析 KV，按「已验证」的 `up.wherecols` 列名取出 WHERE 条件值。
    ///
    /// 对齐 `m_add`/`m_update` 的 `parse_jsdata_kv` 取值约定；某条件列在 jsdata 中缺值则整体报错。
    fn where_values_from_jsdata(&self, up: &UpInfo) -> Result<Vec<Value>, String> {
        if up.wherecols.is_empty() {
            return Ok(vec![]);
        }
        let (jcols, jvals) = Self::parse_jsdata_kv(up)?;
        let kv: std::collections::HashMap<&str, &Value> =
            jcols.iter().zip(jvals.iter()).map(|(c, v)| (c.as_str(), v)).collect();
        let mut values = Vec::with_capacity(up.wherecols.len());
        for col in &up.wherecols {
            let val = kv.get(col.as_str())
                .ok_or_else(|| format!("get 条件列 `{}` 在 jsdata 中无对应值", col))?;
            values.push((*val).clone());
        }
        Ok(values)
    }

    /// 第一步静态校验入口（无需登录 SID）：用控制器已知列白名单校验请求上传的列名，
    /// 把「未验证」字段转换为「已验证」字段，供后续 get/getby_bcid 等业务只读。
    ///
    /// 流程（对齐 TS base78.ts：先控制器定列 → 用已知列校验请求）：
    /// 1. `wherecolsn` 逐列校验 `cols_imp_set` → 通过则写入 `wherecols`；非法则整体报错拒绝。
    /// 2. `getcolsn` 逐列校验 `cols_imp_set` → 通过则写入 `getcols`。
    /// 3. `ordern` 经 `validated_order` 校验（非法回退 `id DESC`）→ 写入 `order`。
    ///
    /// 铁律：业务层只读已验证字段（`wherecols`/`getcols`/`order`/`cid`），绝不直接读
    /// `wherecolsn`/`getcolsn`/`ordern`/`cidn`。未验证字段仅入口/框架内部流转。
    pub fn check_request(&self, up: &mut UpInfo) -> Result<(), String> {
        // 1. WHERE 列名白名单校验（trim "where " 前缀再用裸列名查 cols_imp_set）
        for col in &up.wherecolsn {
            self.validate_query_col(col.trim_start_matches("where "))?;
        }
        up.wherecols = up.wherecolsn.clone();

        // 2. SELECT 返回列名白名单校验
        for col in &up.getcolsn {
            self.validate_query_col(col.trim_start_matches("where "))?;
        }
        up.getcols = up.getcolsn.clone();

        // 3. 排序字段校验（非法回退 id DESC）
        up.order = self.validated_order(&up.ordern);

        Ok(())
    }

    /// 校验查询列名（wherecols / getcols 用的列名），非法则报错
    fn validate_query_col(&self, col: &str) -> Result<(), String> {
        if self.is_system_col(col) {
            return Ok(());
        }
        let lower = col.to_lowercase();
        if !self.cols_imp_set.contains(&lower) {
            return Err(format!("非法列名 `{}`（不在表 `{}` 的列白名单内）", col, self.tbname));
        }
        Ok(())
    }

    /// 校验 ORDER BY 排序字段，防止注入
    ///
    /// 支持形如 `col`、`col DESC`、`col ASC`、`col1 DESC,col2 ASC` 多字段逗号分隔。
    /// 每个字段名必须命中白名单（含系统列），否则整体回退默认 `id DESC`。
    fn validated_order(&self, order: &str) -> String {
        if order.trim().is_empty() {
            return "id DESC".to_string();
        }

        let mut parts = Vec::new();
        for seg in order.split(',') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            // 拆字段名与方向（支持 "col DESC" / "col ASC"）
            let mut tokens = seg.split_whitespace();
            let col = tokens.next().unwrap_or("");
            let dir = tokens.next().unwrap_or("").to_uppercase();
            let dir_ok = if dir.is_empty() {
                true
            } else {
                dir == "ASC" || dir == "DESC"
            };

            let lower = col.to_lowercase();
            let col_ok = self.is_system_col(col) || self.cols_imp_set.contains(&lower);
            if !col_ok || !dir_ok {
                return "id DESC".to_string();
            }

            parts.push(if dir.is_empty() {
                format!("`{}`", col)
            } else {
                format!("`{}` {}", col, dir)
            });
        }

        if parts.is_empty() {
            "id DESC".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// 新增记录（对应 NodeJS Base78.mAdd）
    ///
    /// INSERT INTO `tb` (col1, col2, id, upby, uptime, uidcid) VALUES (?, ?, ?, ?, ?, ?)
    pub async fn m_add(&self, up: &UpInfo) -> Result<String, String> {
        let (cols, vals) = Self::parse_jsdata_kv(up)?;
        let kv = self.validate_cols(&cols, &vals)?;

        let id = datastate::next_id_string();
        let uptime = if up.uptime.is_empty() {
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            up.uptime.clone()
        };
        let upby = up.upby.clone();
        let uidcid_val = up.cid.clone();

        let mut all_cols: Vec<String> = kv.iter().map(|(c, _)| c.clone()).collect();
        all_cols.push("id".to_string());
        all_cols.push("upby".to_string());
        all_cols.push("uptime".to_string());
        all_cols.push(self.uidcid.clone());

        let col_placeholders = vec!["?"; all_cols.len()].join(", ");
        let quoted_cols: Vec<String> = all_cols.iter().map(|c| format!("`{}`", c)).collect();

        let sql = format!(
            "INSERT INTO `{}` ({}) VALUES ({})",
            self.tbname,
            quoted_cols.join(", "),
            col_placeholders
        );

        let mut params: Vec<Value> = kv.into_iter().map(|(_, v)| v).collect();
        params.push(Value::String(id.clone()));
        params.push(Value::String(upby));
        params.push(Value::String(uptime));
        params.push(Value::String(uidcid_val));

        let up_info = datastate::MysqlUpInfo::new();
        let result = self.mysql.do_m_add(&sql, params, &up_info)
            .map_err(|e| format!("新增失败: {}", e))?;
        if let Some(err) = result.error {
            return Err(err);
        }
        Ok(id)
    }

    /// 更新记录（对应 NodeJS Base78.mUpdate，id 取自 up.mid）
    ///
    /// UPDATE `tb` SET col=?, upby=?, uptime=? WHERE id=? AND uidcid=?
    pub async fn m_update(&self, up: &UpInfo) -> Result<i64, String> {
        let (cols, vals) = Self::parse_jsdata_kv(up)?;
        let kv = self.validate_cols(&cols, &vals)?;
        self.do_update(up, &kv).await
    }

    /// 按 id 更新（对应 NodeJS Base78.mUpdateByid）
    pub async fn m_update_byid(&self, up: &UpInfo) -> Result<i64, String> {
        let (cols, vals) = Self::parse_jsdata_kv(up)?;
        let kv = self.validate_cols(&cols, &vals)?;
        self.do_update(up, &kv).await
    }

    /// 更新内部实现
    async fn do_update(&self, up: &UpInfo, kv: &[(String, Value)]) -> Result<i64, String> {
        let id = up.mid.clone();
        if id.is_empty() {
            return Err("缺少 mid（记录 id）".to_string());
        }

        let uptime = if up.uptime.is_empty() {
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            up.uptime.clone()
        };
        let upby = up.upby.clone();

        let mut set_clauses: Vec<String> = kv.iter().map(|(c, _)| format!("`{}` = ?", c)).collect();
        set_clauses.push("`upby` = ?".to_string());
        set_clauses.push("`uptime` = ?".to_string());

        let sql = format!(
            "UPDATE `{}` SET {} WHERE `id` = ? AND `{}` = ?",
            self.tbname,
            set_clauses.join(", "),
            self.uidcid
        );

        let mut params: Vec<Value> = kv.iter().map(|(_, v)| v.clone()).collect();
        params.push(Value::String(upby));
        params.push(Value::String(uptime));
        params.push(Value::String(id));
        params.push(Value::String(up.cid.clone()));

        let up_info = datastate::MysqlUpInfo::new();
        let result = self.mysql.do_m(&sql, params, &up_info)
            .map_err(|e| format!("更新失败: {}", e))?;
        if let Some(err) = result.error {
            return Err(err);
        }
        Ok(result.affected_rows)
    }

    /// 删除记录（对应 NodeJS Base78.mdel，id 取自 up.mid）
    ///
    /// DELETE FROM `tb` WHERE id=? AND uidcid=?
    pub async fn m_del(&self, up: &UpInfo) -> Result<i64, String> {
        let id = up.mid.clone();
        if id.is_empty() {
            return Err("缺少 mid（记录 id）".to_string());
        }

        let sql = format!(
            "DELETE FROM `{}` WHERE `id` = ? AND `{}` = ?",
            self.tbname, self.uidcid
        );
        let params = vec![Value::String(id), Value::String(up.cid.clone())];

        let up_info = datastate::MysqlUpInfo::new();
        let result = self.mysql.do_m(&sql, params, &up_info)
            .map_err(|e| format!("删除失败: {}", e))?;
        if let Some(err) = result.error {
            return Err(err);
        }
        Ok(result.affected_rows)
    }

    /// 批量删除（对应 NodeJS Base78.mdelmany，id 列表从 jsdata 解析）
    ///
    /// jsdata 为 id 数组：["id1","id2",...]
    /// DELETE FROM `tb` WHERE id IN (...) AND uidcid=?
    pub async fn m_del_many(&self, up: &UpInfo) -> Result<i64, String> {
        let jsdata = up.jsdata.as_deref().ok_or("缺少 jsdata 参数")?;
        let ids: Vec<String> = serde_json::from_str::<Vec<String>>(jsdata)
            .map_err(|e| format!("jsdata 需为 id 字符串数组: {}", e))?;
        if ids.is_empty() {
            return Err("id 列表为空".to_string());
        }

        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "DELETE FROM `{}` WHERE `id` IN ({}) AND `{}` = ?",
            self.tbname, placeholders, self.uidcid
        );

        let mut params: Vec<Value> = ids.into_iter().map(Value::String).collect();
        params.push(Value::String(up.cid.clone()));

        let up_info = datastate::MysqlUpInfo::new();
        let result = self.mysql.do_m(&sql, params, &up_info)
            .map_err(|e| format!("批量删除失败: {}", e))?;
        if let Some(err) = result.error {
            return Err(err);
        }
        Ok(result.affected_rows)
    }

    /// upsert（对应 NodeJS Base78.m，按 up.mid 定位：存在则 update，不存在则 add）
    pub async fn m(&self, up: &UpInfo) -> Result<Value, String> {
        let id = up.mid.clone();
        if id.is_empty() {
            // 无 id，直接新增
            let new_id = self.m_add(up).await?;
            return Ok(Value::String(new_id));
        }

        // 先查是否已存在
        let sql = format!(
            "SELECT * FROM `{}` WHERE `id` = ? AND `{}` = ? LIMIT 1",
            self.tbname, self.uidcid
        );
        let up_info = datastate::MysqlUpInfo::new();
        let existing = self.mysql
            .do_get(&sql, vec![Value::String(id.clone()), Value::String(up.cid.clone())], &up_info)
            .map_err(|e| format!("查询失败: {}", e))?;

        if existing.is_empty() {
            let new_id = self.m_add(up).await?;
            return Ok(Value::String(new_id));
        }

        let affected = self.m_update(up).await?;
        Ok(Value::Number(affected.into()))
    }
}

/// MysqlCidBase78 - MySQL 版基于 CID 隔离的控制器基类
pub struct MysqlCidBase78 {
    pub base: MysqlBase78,
}

impl MysqlCidBase78 {
    pub fn new(tbname: &str, mysql: Arc<Mysql78>, cols_imp: Vec<String>) -> Self {
        Self {
            base: MysqlBase78::new(tbname, "cid", mysql, cols_imp),
        }
    }

    pub async fn get(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.get(up).await
    }

    pub async fn do_get(&self, sql: &str, params: Vec<Value>) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.do_get(sql, params).await
    }

    pub async fn getby_bcid(&self, up: &UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        self.base.getby_bcid(up).await
    }

    pub fn set_allow_bcid(&mut self) {
        self.base.set_allow_bcid();
    }

    pub async fn m_add(&self, up: &UpInfo) -> Result<String, String> {
        self.base.m_add(up).await
    }

    pub async fn m_update(&self, up: &UpInfo) -> Result<i64, String> {
        self.base.m_update(up).await
    }

    pub async fn m_update_byid(&self, up: &UpInfo) -> Result<i64, String> {
        self.base.m_update_byid(up).await
    }

    pub async fn m_del(&self, up: &UpInfo) -> Result<i64, String> {
        self.base.m_del(up).await
    }

    pub async fn m_del_many(&self, up: &UpInfo) -> Result<i64, String> {
        self.base.m_del_many(up).await
    }

    pub async fn m(&self, up: &UpInfo) -> Result<Value, String> {
        self.base.m(up).await
    }

    /// 内置 call 分发（完整 CRUD），子类无需手写
    async fn _call(&self, up: &mut UpInfo, fun: &str) -> Value {
        let up_clone = up.clone();
        let fun_lower = fun.to_lowercase();
        let result: (StatusCode, Bytes) = match fun_lower.as_str() {
            "get" => {
                match self.get(&up_clone).await {
                    Ok(rows) => {
                        let arr: Vec<Value> = rows.iter()
                            .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                            .collect();
                        let resp = Response::success_json(&Value::Array(arr));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "getbybcid" => {
                match self.getby_bcid(&up_clone).await {
                    Ok(rows) => {
                        let arr: Vec<Value> = rows.iter()
                            .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                            .collect();
                        let resp = Response::success_json(&Value::Array(arr));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -403);
                        (StatusCode::FORBIDDEN, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "madd" => {
                match self.m_add(&up_clone).await {
                    Ok(id) => {
                        let resp = Response::success_json(&serde_json::json!({ "id": id }));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "mupdate" | "mupdatebyid" => {
                match self.m_update(&up_clone).await {
                    Ok(affected) => {
                        let resp = Response::success_json(&serde_json::json!({ "affected": affected }));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "mdel" => {
                match self.m_del(&up_clone).await {
                    Ok(affected) => {
                        let resp = Response::success_json(&serde_json::json!({ "affected": affected }));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "mdelmany" => {
                match self.m_del_many(&up_clone).await {
                    Ok(affected) => {
                        let resp = Response::success_json(&serde_json::json!({ "affected": affected }));
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            "m" | "mbyid" => {
                match self.m(&up_clone).await {
                    Ok(v) => {
                        let resp = Response::success_json(&v);
                        (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                    Err(e) => {
                        let resp = Response::fail(&e, -1);
                        (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
                    }
                }
            }
            _ => {
                let resp = Response::fail(&format!("API not found: {}", fun), 404);
                (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
            }
        };

        let resp: Value = serde_json::from_slice(&result.1).unwrap_or(Value::Null);
        if result.0 != StatusCode::OK {
            up.res = -1;
            up.errmsg = resp.get("errmsg").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return Value::Null;
        }
        resp.get("back").and_then(|v| {
            if let Some(s) = v.as_str() {
                serde_json::from_str(s).ok()
            } else {
                Some(v.clone())
            }
        }).unwrap_or(Value::Null)
    }
}

#[async_trait]
impl Controller78 for MysqlCidBase78 {
    async fn call(&self, up: &mut UpInfo, fun: &str, _method: &Method) -> Value {
        self._call(up, fun).await
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

    // ============ MysqlBase78 改版测试 ============

    fn mysql_base(cols_imp: Vec<&str>) -> MysqlBase78 {
        let mysql = Arc::new(Mysql78::default());
        MysqlBase78::new(
            "test_tb",
            "cid",
            mysql,
            cols_imp.iter().map(|s| s.to_string()).collect(),
        )
    }

    // 列名验证在 crud 方法内部完成（build_where / build_where_bcid），对齐 base78.ts。
    // 以下测试覆盖 wherecols（条件列）+ jsdata 解析值（值）的白名单校验。

    #[test]
    fn test_build_where_no_cols() {
        let base = mysql_base(vec!["name", "phone"]);
        let (clause, params) = base.build_where(&[], &[], "cid123").unwrap();
        assert_eq!(clause, "`cid` = ?");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::String("cid123".to_string()));
    }

    #[test]
    fn test_build_where_normal() {
        let base = mysql_base(vec!["name", "phone"]);
        let cols = vec!["name".to_string(), "phone".to_string()];
        let pars = vec![Value::String("alice".to_string()), Value::String("123".to_string())];
        let (clause, params) = base.build_where(&cols, &pars, "cid123").unwrap();
        assert_eq!(clause, "`cid` = ? AND `name` = ? AND `phone` = ?");
        assert_eq!(params.len(), 3);
        assert_eq!(params[1], Value::String("alice".to_string()));
        assert_eq!(params[2], Value::String("123".to_string()));
    }

    #[test]
    fn test_build_where_pars_truncated() {
        let base = mysql_base(vec!["name", "phone"]);
        let cols = vec!["name".to_string(), "phone".to_string(), "extra".to_string()];
        let pars = vec![Value::String("alice".to_string())];
        let (clause, params) = base.build_where(&cols, &pars, "cid123").unwrap();
        // pars 只有 1 个，截断对齐：只拼一个条件（对齐 TS colp.slice(0, pars.length)）
        assert_eq!(clause, "`cid` = ? AND `name` = ?");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_where_illegal_col() {
        let base = mysql_base(vec!["name", "phone"]);
        let cols = vec!["name".to_string(), "evil".to_string()];
        let pars = vec![Value::String("alice".to_string()), Value::String("x".to_string())];
        let result = base.build_where(&cols, &pars, "cid123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("非法列名"));
    }

    #[test]
    fn test_build_where_system_col_allowed() {
        let base = mysql_base(vec!["name", "phone"]);
        let cols = vec!["id".to_string(), "uptime".to_string()];
        let pars = vec![Value::String("1".to_string()), Value::String("t".to_string())];
        let result = base.build_where(&cols, &pars, "cid123");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_getby_bcid_not_allowed_by_default() {
        let base = mysql_base(vec!["name"]);
        let mut up = UpInfo::new();
        up.bcid = "some-bcid".to_string();
        let result = base.getby_bcid(&up).await;
        // 默认 allow_bcid=false，应拒绝（越权保护），不访问数据库
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_getby_bcid_missing_bcid() {
        let mut base = mysql_base(vec!["name"]);
        base.set_allow_bcid();
        let up = UpInfo::new(); // bcid 为空
        let result = base.getby_bcid(&up).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("缺少 bcid"));
    }

    #[test]
    fn test_build_where_bcid_illegal_col() {
        let base = mysql_base(vec!["name", "phone"]);
        let cols = vec!["evil".to_string()];
        let pars = vec![Value::String("x".to_string())];
        let result = base.build_where_bcid(&cols, &pars, "bcid123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("非法列名"));
    }
}
