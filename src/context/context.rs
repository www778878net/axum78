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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_body_default() {
        let body = RequestBody::default();
        assert!(body.sid.is_empty());
        assert!(body.pars.is_empty());
        assert!(body.cols.is_empty());
        assert!(body.mid.is_empty());
        assert!(body.midpk.is_none());
        assert!(body.order.is_empty());
        assert!(body.start.is_none());
        assert!(body.number.is_none());
    }

    #[test]
    fn test_request_body_from_json_valid() {
        let json = r#"{
            "sid": "test_sid",
            "pars": [1, "text", true],
            "cols": ["col1", "col2"],
            "mid": "mid123",
            "midpk": 100,
            "order": "id DESC",
            "start": 0,
            "number": 10
        }"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.sid, "test_sid");
        assert_eq!(body.pars.len(), 3);
        assert_eq!(body.cols.len(), 2);
        assert_eq!(body.mid, "mid123");
        assert_eq!(body.midpk, Some(100));
        assert_eq!(body.order, "id DESC");
        assert_eq!(body.start, Some(0));
        assert_eq!(body.number, Some(10));
    }

    #[test]
    fn test_request_body_from_json_partial() {
        let json = r#"{"sid": "test_sid"}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.sid, "test_sid");
        assert!(body.pars.is_empty());
        assert!(body.cols.is_empty());
    }

    #[test]
    fn test_request_body_from_json_empty() {
        let json = r#"{}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert!(body.sid.is_empty());
        assert!(body.pars.is_empty());
    }

    #[test]
    fn test_request_body_from_json_invalid() {
        let json = r#"invalid json"#;

        let result = RequestBody::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_body_from_json_with_array_pars() {
        let json = r#"{"pars": [{"key": "value"}, [1, 2, 3]]}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.pars.len(), 2);
    }
}
