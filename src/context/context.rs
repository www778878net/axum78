//! 请求上下文 UpInfo - 对应 koa78-base78 的 UpInfo
//!
//! 从 HTTP 请求中解析 API 路径、参数、认证信息等

use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::Local;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// API 请求上下文 - 对应 koa78-base78 的 UpInfo
///
/// 从 4 级路由 `/:apisys/:apimicro/:apiobj/:apifun` 和请求体中提取
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpInfo {
    // ========== 4级路由参数 ==========
    /// API 系统 (必须以 "api" 开头)
    pub apisys: String,
    /// API 微服务/模块
    pub apimicro: String,
    /// API 对象/控制器
    pub apiobj: String,
    /// API 函数/方法
    pub apifun: String,

    // ========== 认证与隔离 ==========
    /// 会话 ID (用于认证)
    #[serde(default)]
    pub sid: String,
    /// 公司 ID (多租户隔离)
    #[serde(default)]
    pub cid: String,
    /// 用户 ID
    #[serde(default)]
    pub uid: String,

    // ========== 请求参数 ==========
    /// 方法参数数组
    #[serde(default)]
    pub pars: Vec<Value>,
    /// 列名 (用于选择性查询)
    #[serde(default)]
    pub cols: Vec<String>,
    /// 记录 UUID (用于更新/删除)
    #[serde(default)]
    pub mid: String,
    /// 记录主键 (自增ID)
    #[serde(default)]
    pub midpk: i64,

    // ========== 分页与排序 ==========
    /// 排序
    #[serde(default)]
    pub order: String,
    /// 起始位置
    #[serde(default)]
    pub start: i64,
    /// 数量
    #[serde(default)]
    pub number: i64,

    // ========== 返回类型 ==========
    /// 返回类型 (如 "protobuf")
    #[serde(default)]
    pub backtype: String,

    // ========== 上下文信息 ==========
    /// 请求时间
    #[serde(default)]
    pub uptime: String,
    /// 请求 ID (追踪用)
    #[serde(default)]
    pub req_id: String,
    /// 是否调试模式
    #[serde(default)]
    pub debug: bool,

    // ========== 响应状态 (控制器设置) ==========
    /// 结果码: 0 成功
    #[serde(default)]
    pub res: i32,
    /// 错误信息
    #[serde(default)]
    pub errmsg: String,
}

/// 请求体格式 - 对应 logsvc POST 请求体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestBody {
    #[serde(default)]
    pub sid: String,
    #[serde(default)]
    pub pars: Vec<Value>,
    #[serde(default)]
    pub cols: Vec<String>,
    #[serde(default)]
    pub mid: String,
    #[serde(default)]
    pub midpk: Option<i64>,
    #[serde(default)]
    pub order: String,
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub number: Option<i64>,
}

impl UpInfo {
    /// 创建新上下文
    pub fn new() -> Self {
        Self {
            uptime: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            req_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            start: 0,
            number: 20,
            ..Default::default()
        }
    }

    /// 从 4 级路由参数创建
    pub fn from_route(apisys: &str, apimicro: &str, apiobj: &str, apifun: &str) -> Self {
        let mut up = Self::new();
        up.apisys = apisys.to_string();
        up.apimicro = apimicro.to_string();
        up.apiobj = apiobj.to_string();
        up.apifun = apifun.to_string();
        up
    }

    /// 从请求体填充参数
    pub fn fill_from_body(&mut self, body: &RequestBody) {
        if !body.sid.is_empty() {
            // 从 sid 提取 cid: 格式为 "cid" 或 "cid|other"
            self.sid = body.sid.clone();
            if let Some(cid) = self.sid.split('|').next() {
                self.cid = cid.to_string();
            }
        }
        if !body.pars.is_empty() {
            self.pars = body.pars.clone();
        }
        if !body.cols.is_empty() {
            self.cols = body.cols.clone();
        }
        if !body.mid.is_empty() {
            self.mid = body.mid.clone();
        }
        if let Some(midpk) = body.midpk {
            self.midpk = midpk;
        }
        if !body.order.is_empty() {
            self.order = body.order.clone();
        }
        if let Some(start) = body.start {
            self.start = start;
        }
        if let Some(number) = body.number {
            self.number = number;
        }
    }

    /// 设置返回类型
    pub fn set_backtype(&mut self, backtype: &str) {
        self.backtype = backtype.to_string();
    }

    /// 设置成功
    pub fn set_ok(&mut self) {
        self.res = 0;
        self.errmsg = String::new();
    }

    /// 设置错误
    pub fn set_error(&mut self, code: i32, errmsg: &str) {
        self.res = code;
        self.errmsg = errmsg.to_string();
    }

    /// 生成新 ID (业务主键) - 对应 logsvc 的 ID 生成
    pub fn new_id() -> String {
        let ts = Local::now().format("%Y%m%d%H%M%S").to_string();
        let suffix = uuid::Uuid::new_v4().to_string()[..6].to_string();
        format!("{}{}", ts, suffix)
    }

    /// 创建默认上下文 (用于测试)
    pub fn default_context() -> Self {
        let mut up = Self::new();
        up.cid = "default".to_string();
        up.uid = "test".to_string();
        up.apisys = "apitest".to_string();
        up.apimicro = "testmenu".to_string();
        up.apiobj = "testtb".to_string();
        up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upinfo_new() {
        let up = UpInfo::new();
        assert!(up.cid.is_empty());
        assert!(!up.req_id.is_empty());
        assert_eq!(up.start, 0);
        assert_eq!(up.number, 20);
    }

    #[test]
    fn test_upinfo_from_route() {
        let up = UpInfo::from_route("apitest", "testmenu", "testtb", "get");
        assert_eq!(up.apisys, "apitest");
        assert_eq!(up.apimicro, "testmenu");
        assert_eq!(up.apiobj, "testtb");
        assert_eq!(up.apifun, "get");
    }

    #[test]
    fn test_upinfo_fill_from_body() {
        let mut up = UpInfo::from_route("apitest", "testmenu", "testtb", "get");
        let body = RequestBody {
            sid: "mycid|extra".to_string(),
            pars: vec![serde_json::json!("value1"), serde_json::json!(123)],
            mid: "record-id".to_string(),
            start: Some(10),
            number: Some(50),
            ..Default::default()
        };
        up.fill_from_body(&body);
        assert_eq!(up.sid, "mycid|extra");
        assert_eq!(up.cid, "mycid");
        assert_eq!(up.pars.len(), 2);
        assert_eq!(up.mid, "record-id");
        assert_eq!(up.start, 10);
        assert_eq!(up.number, 50);
    }

    #[test]
    fn test_upinfo_default_context() {
        let up = UpInfo::default_context();
        assert_eq!(up.cid, "default");
        assert_eq!(up.uid, "test");
    }
}