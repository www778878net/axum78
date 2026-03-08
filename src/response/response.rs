//! API 响应格式 - 统一响应结构

use serde::{Deserialize, Serialize};
use axum::{
    http::StatusCode,
    Json,
};

/// 统一 API 响应格式
///
/// 对应 koa78-base78 的响应格式: { res, errmsg, data }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// 结果码: 0 成功, 负数失败
    pub res: i32,
    /// 错误信息
    pub errmsg: String,
    /// 响应数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    /// 成功响应
    pub fn success(data: T) -> Self {
        Self {
            res: 0,
            errmsg: String::new(),
            data: Some(data),
        }
    }

    /// 成功响应 (无数据)
    pub fn ok() -> ApiResponse<()> {
        ApiResponse {
            res: 0,
            errmsg: String::new(),
            data: None,
        }
    }

    /// 失败响应
    pub fn fail(errmsg: &str, code: i32) -> Self {
        Self {
            res: code,
            errmsg: errmsg.to_string(),
            data: None,
        }
    }

    /// 转换为 Axum JSON 响应
    pub fn into_json(self) -> Json<Self> {
        Json(self)
    }

    /// 转换为 Axum HTTP 响应
    pub fn into_response(self) -> (StatusCode, Json<Self>) {
        let status = if self.res == 0 {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        };
        (status, Json(self))
    }
}

/// API 错误
#[derive(Debug)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

impl ApiError {
    pub fn new(message: &str, code: i32) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

// 常用错误码 (公共 API，供外部使用)
#[allow(dead_code)]
pub const ERR_NOT_FOUND: i32 = -1;
#[allow(dead_code)]
pub const ERR_PARAM: i32 = -2;
#[allow(dead_code)]
pub const ERR_DB: i32 = -3;
#[allow(dead_code)]
pub const ERR_AUTH: i32 = -4;
#[allow(dead_code)]
pub const ERR_EXISTS: i32 = -5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let resp = ApiResponse::success("test data");
        assert_eq!(resp.res, 0);
        assert!(resp.errmsg.is_empty());
        assert_eq!(resp.data, Some("test data"));
    }

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::<()>::ok();
        assert_eq!(resp.res, 0);
        assert!(resp.data.is_none());
    }

    #[test]
    fn test_api_response_fail() {
        let resp = ApiResponse::<()>::fail("error msg", -1);
        assert_eq!(resp.res, -1);
        assert_eq!(resp.errmsg, "error msg");
    }
}