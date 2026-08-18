//! 请求上下文 UpInfo - 对应 koa78-base78 的 UpInfo
//!
//! 直接重导出 base::UpInfo

// 重导出 base::UpInfo
pub use crate::base::UpInfo;

/// 请求体格式 - 对应 logsvc POST 请求体
///
/// 对齐 base::UpInfo 的字段分离范式：
/// - 上传原始「未验证」值（`wherecolsn`/`getcolsn`/`ordern`/`jsdata`），由 Base78::check_request 校验后写入已验证字段。
/// - 旧 wire 字段名 `pars`/`cols` 已弃用，不再兼容（类型不符：`pars` 为数组、`jsdata` 为 JSON 串；`cols` 为弃用上传列名）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RequestBody {
    #[serde(default)]
    pub sid: String,
    /// 上传原始 WHERE 列名（未验证），经 check_request 校验后写入 wherecols
    #[serde(default)]
    pub wherecolsn: Vec<String>,
    /// 上传原始 SELECT 列名（未验证），经 check_request 校验后写入 getcols
    #[serde(default)]
    pub getcolsn: Vec<String>,
    /// 上传原始排序字段（未验证），经 check_request 校验后写入 order
    #[serde(default, alias = "order")]
    pub ordern: String,
    /// 条件值 / 写入数据，JSON 对象字符串（对齐 m_add/m_update）
    #[serde(default)]
    pub jsdata: Option<String>,
    #[serde(default)]
    pub mid: String,
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
        assert!(body.wherecolsn.is_empty());
        assert!(body.getcolsn.is_empty());
        assert!(body.jsdata.is_none());
        assert!(body.mid.is_empty());
        assert!(body.ordern.is_empty());
        assert!(body.start.is_none());
        assert!(body.number.is_none());
    }

    #[test]
    fn test_request_body_from_json_valid() {
        let json = r#"{
            "sid": "test_sid",
            "wherecolsn": ["col1", "col2"],
            "getcolsn": ["col1"],
            "jsdata": "{\"key\":\"value\"}",
            "mid": "mid123",
            "ordern": "id DESC",
            "start": 0,
            "number": 10
        }"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.sid, "test_sid");
        assert_eq!(body.wherecolsn.len(), 2);
        assert_eq!(body.getcolsn.len(), 1);
        assert_eq!(body.jsdata, Some(r#"{"key":"value"}"#.to_string()));
        assert_eq!(body.mid, "mid123");
        assert_eq!(body.ordern, "id DESC");
        assert_eq!(body.start, Some(0));
        assert_eq!(body.number, Some(10));
    }

    #[test]
    fn test_request_body_from_json_partial() {
        let json = r#"{"sid": "test_sid"}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.sid, "test_sid");
        assert!(body.wherecolsn.is_empty());
        assert!(body.getcolsn.is_empty());
    }

    #[test]
    fn test_request_body_from_json_empty() {
        let json = r#"{}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert!(body.sid.is_empty());
        assert!(body.wherecolsn.is_empty());
    }

    #[test]
    fn test_request_body_from_json_invalid() {
        let json = r#"invalid json"#;

        let result = RequestBody::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_body_from_json_with_getcolsn() {
        let json = r#"{"wherecolsn": ["a", "b"], "getcolsn": ["c"], "jsdata": "{\"x\":1}", "ordern": "id ASC"}"#;

        let body = RequestBody::from_json(json).unwrap();
        assert_eq!(body.wherecolsn, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(body.getcolsn, vec!["c".to_string()]);
        assert_eq!(body.jsdata, Some(r#"{"x":1}"#.to_string()));
        assert_eq!(body.ordern, "id ASC");
    }
}