//! 请求上下文 - 类似 koa78-base78 的 UpInfo

use serde::{Deserialize, Serialize};
use chrono::Local;

/// API 请求上下文
///
/// 对应 Node.js 版本的 UpInfo，包含请求的元信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Context {
    /// 公司 ID (数据隔离)
    pub cid: String,
    /// 用户 ID
    pub uid: String,
    /// 用户名
    pub uname: String,
    /// 请求时间
    pub uptime: String,
    /// 请求 ID (追踪用)
    pub req_id: String,
    /// 是否调试模式
    pub debug: bool,
    /// API 路径: apisys/apimicro/apiobj
    pub apisys: String,
    pub apimicro: String,
    pub apiobj: String,
}

impl Context {
    /// 创建新上下文
    pub fn new() -> Self {
        Self {
            cid: String::new(),
            uid: String::new(),
            uname: String::new(),
            uptime: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            req_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            debug: false,
            apisys: String::new(),
            apimicro: String::new(),
            apiobj: String::new(),
        }
    }

    /// 创建默认上下文 (用于测试)
    pub fn default_context() -> Self {
        Self {
            cid: "default".to_string(),
            uid: "test".to_string(),
            uname: "tester".to_string(),
            uptime: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            req_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            debug: false,
            apisys: "api".to_string(),
            apimicro: "basic".to_string(),
            apiobj: "test".to_string(),
        }
    }

    /// 生成新 ID (业务主键)
    pub fn new_id() -> String {
        let ts = Local::now().format("%Y%m%d%H%M%S").to_string();
        let suffix = uuid::Uuid::new_v4().to_string()[..6].to_string();
        format!("{}{}", ts, suffix)
    }

    /// 设置 API 路径
    pub fn with_api(mut self, apisys: &str, apimicro: &str, apiobj: &str) -> Self {
        self.apisys = apisys.to_string();
        self.apimicro = apimicro.to_string();
        self.apiobj = apiobj.to_string();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let ctx = Context::new();
        assert!(ctx.cid.is_empty());
        assert!(!ctx.req_id.is_empty());
    }

    #[test]
    fn test_context_default_context() {
        let ctx = Context::default_context();
        assert_eq!(ctx.cid, "default");
        assert_eq!(ctx.uid, "test");
    }

    #[test]
    fn test_context_with_api() {
        let ctx = Context::new().with_api("test", "user", "get");
        assert_eq!(ctx.apisys, "test");
        assert_eq!(ctx.apimicro, "user");
        assert_eq!(ctx.apiobj, "get");
    }
}