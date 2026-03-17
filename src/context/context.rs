//! 请求上下文 UpInfo - 对应 koa78-base78 的 UpInfo
//!
//! 直接重导出 base::UpInfo

// 重导出 base::UpInfo
pub use base::UpInfo;

/// 请求体格式 - 对应 logsvc POST 请求体
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RequestBody {
    #[serde(default)]
    pub sid: String,
    #[serde(default)]
    pub pars: Vec<serde_json::Value>,
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

impl RequestBody {
    /// 从 JSON 解析
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
