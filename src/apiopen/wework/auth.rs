//! WeWork (Enterprise WeChat) Authentication
//!
//! Route: /apimes/wework/auth/:apifun
//!
//! Functions:
//! - login: Redirect to WeWork OAuth
//! - callback: Handle OAuth callback and get user info

use axum::{
    body::Bytes,
    http::StatusCode,
};
use crate::base::{UpInfo, Response, ProjectPath};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// WeWork Configuration
#[derive(Debug, Clone, Default)]
pub struct WeWorkConfig {
    pub corp_id: String,
    pub corp_secret: String,
    pub agent_id: String,
    pub admin_userid: String,
    pub chatid_jhs: String,
    pub chatid_admin: String,
    pub default_to_user: String,
    pub token: String,
    pub encoding_aes_key: String,
    pub safe: i32,
    pub token_cache_seconds: i64,
}

static WEWORK_CONFIG: OnceLock<WeWorkConfig> = OnceLock::new();

/// Load WeWork config from ini file (环境变量优先，其次读 docs/config/{env}.ini)
///
/// 敏感凭证（corp_secret/token/encoding_aes_key 等）不再硬编码，
/// 统一通过 `ProjectPath::read_ini_value` 读取：
/// - 环境变量优先（key 转大写，如 `corp_id` → `CORP_ID`）
/// - 无环境变量时读 `docs/config/{APP_ENV}.ini` 的 `[wework]` 段
pub fn get_wework_config() -> &'static WeWorkConfig {
    WEWORK_CONFIG.get_or_init(|| {
        let project = ProjectPath::find().unwrap_or_default();
        let get = |key: &str| -> String {
            project.read_ini_value("wework", key).unwrap_or_default()
        };

        let safe = get("safe").parse::<i32>().unwrap_or(0);
        let token_cache_seconds = get("token_cache_seconds").parse::<i64>().unwrap_or(7200);

        let config = WeWorkConfig {
            corp_id: get("corp_id"),
            corp_secret: get("corp_secret"),
            agent_id: get("agent_id"),
            admin_userid: get("admin_userid"),
            chatid_jhs: get("chatid_jhs"),
            chatid_admin: get("chatid_admin"),
            default_to_user: if get("default_to_user").is_empty() {
                "@all".to_string()
            } else {
                get("default_to_user")
            },
            token: get("token"),
            encoding_aes_key: get("encoding_aes_key"),
            safe,
            token_cache_seconds,
        };

        // 启动时检测缺失的关键配置，记录告警（不 panic）
        if config.corp_id.is_empty() || config.corp_secret.is_empty() || config.agent_id.is_empty() {
            tracing::error!(
                "企微配置缺失：corp_id/agent_id/corp_secret 未在环境变量或 docs/config/*.ini 的 [wework] 段中配置"
            );
        }

        config
    })
}

/// WeWork API response for access token
#[derive(Debug, Serialize, Deserialize)]
struct AccessTokenResponse {
    errcode: i32,
    errmsg: String,
    access_token: Option<String>,
    expires_in: Option<i32>,
}

/// WeWork API response for user info
#[derive(Debug, Serialize, Deserialize)]
struct UserInfoResponse {
    errcode: i32,
    errmsg: String,
    userid: Option<String>,
    name: Option<String>,
    avatar: Option<String>,
    mobile: Option<String>,
    email: Option<String>,
}

/// WeWork API response for user detail
#[derive(Debug, Serialize, Deserialize)]
struct UserDetailResponse {
    errcode: i32,
    errmsg: String,
    userid: Option<String>,
    name: Option<String>,
    department: Option<Vec<i64>>,
    position: Option<String>,
    mobile: Option<String>,
    email: Option<String>,
    avatar: Option<String>,
    status: Option<i32>,
}

/// Handle WeWork auth requests
pub async fn handle(apifun: &str, up: UpInfo) -> (StatusCode, Bytes) {
    match apifun.to_lowercase().as_str() {
        "login" => login(&up).await,
        "callback" => callback(&up).await,
        "gettoken" => get_token(&up).await,
        "getuser" => get_user(&up).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// Handle raw HTTP request (no middleware)
pub async fn handle_raw(apifun: &str, body: Bytes) -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 1], Bytes) {
    // Parse body as JSON to get UpInfo
    let up: UpInfo = if body.is_empty() {
        UpInfo::new()
    } else {
        match serde_json::from_slice(&body) {
            Ok(min) => {
                let min: MinimalRequest = min;
                min.into()
            }
            Err(_) => UpInfo::new()
        }
    };
    
    let (status, bytes) = handle(apifun, up).await;
    (status, [(axum::http::header::CONTENT_TYPE, "application/json")], bytes)
}

/// Minimal request for parsing
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct MinimalRequest {
    #[serde(default)]
    sid: String,
    #[serde(default)]
    cid: String,
    #[serde(default)]
    uid: String,
    #[serde(default)]
    uname: String,
    #[serde(default)]
    jsdata: Option<String>,
}

impl Default for MinimalRequest {
    fn default() -> Self {
        Self {
            sid: String::new(),
            cid: String::new(),
            uid: String::new(),
            uname: "guest".to_string(),
            jsdata: None,
        }
    }
}

impl From<MinimalRequest> for UpInfo {
    fn from(min: MinimalRequest) -> Self {
        let mut up = UpInfo::new();
        up.sid = min.sid;
        up.cid = min.cid;
        up.uid = min.uid;
        up.uname = min.uname;
        up.jsdata = min.jsdata;
        up
    }
}

/// Get WeWork OAuth login URL
async fn login(up: &UpInfo) -> (StatusCode, Bytes) {
    let config = get_wework_config();
    
    if config.corp_id.is_empty() || config.agent_id.is_empty() {
        let resp = Response::fail("WeWork config not set", -1);
        return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }
    
    // Generate state from sid for security
    let state = if up.sid.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        up.sid.clone()
    };
    
    // Return config info, frontend should build OAuth URL with its callback
    let resp = Response::success_json(&serde_json::json!({
        "corp_id": config.corp_id,
        "agent_id": config.agent_id,
        "state": state
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// Handle OAuth callback
async fn callback(up: &UpInfo) -> (StatusCode, Bytes) {
    let config = get_wework_config();
    
    // Get code from jsdata
    let code = match &up.jsdata {
        Some(data) => {
            match serde_json::from_str::<serde_json::Value>(data) {
                Ok(json) => json.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                Err(_) => {
                    let resp = Response::fail("Invalid jsdata format", -1);
                    return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
                }
            }
        }
        None => {
            let resp = Response::fail("Missing code parameter", -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    if code.is_empty() {
        let resp = Response::fail("Code is empty", -1);
        return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }
    
    // Get access token
    let access_token = match get_access_token(&config).await {
        Ok(token) => token,
        Err(e) => {
            let resp = Response::fail(&format!("Get access token failed: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    // Get user info with code
    let user_info = match get_user_info(&access_token, &code).await {
        Ok(info) => info,
        Err(e) => {
            let resp = Response::fail(&format!("Get user info failed: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    let resp = Response::success_json(&serde_json::json!({
        "userid": user_info.userid,
        "name": user_info.name,
        "avatar": user_info.avatar,
        "mobile": user_info.mobile,
    }));
    (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// Get access token (for testing/debugging)
async fn get_token(up: &UpInfo) -> (StatusCode, Bytes) {
    let config = get_wework_config();
    
    match get_access_token(&config).await {
        Ok(token) => {
            let resp = Response::success_json(&serde_json::json!({
                "access_token": token
            }));
            (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
        Err(e) => {
            let resp = Response::fail(&format!("Get access token failed: {}", e), -1);
            (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// Get user info by userid (for testing/debugging)
async fn get_user(up: &UpInfo) -> (StatusCode, Bytes) {
    let config = get_wework_config();
    
    let userid = match &up.jsdata {
        Some(data) => {
            match serde_json::from_str::<serde_json::Value>(data) {
                Ok(json) => json.get("userid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                Err(_) => {
                    let resp = Response::fail("Invalid jsdata format", -1);
                    return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
                }
            }
        }
        None => {
            let resp = Response::fail("Missing userid parameter", -1);
            return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    if userid.is_empty() {
        let resp = Response::fail("Userid is empty", -1);
        return (StatusCode::BAD_REQUEST, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }
    
    let access_token = match get_access_token(&config).await {
        Ok(token) => token,
        Err(e) => {
            let resp = Response::fail(&format!("Get access token failed: {}", e), -1);
            return (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    match get_user_detail(&access_token, &userid).await {
        Ok(user) => {
            let resp = Response::success_json(&serde_json::json!({
                "userid": user.userid,
                "name": user.name,
                "mobile": user.mobile,
                "email": user.email,
                "avatar": user.avatar,
                "department": user.department,
                "position": user.position,
            }));
            (StatusCode::OK, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
        Err(e) => {
            let resp = Response::fail(&format!("Get user detail failed: {}", e), -1);
            (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// Get access token from WeWork API
async fn get_access_token(config: &WeWorkConfig) -> Result<String, String> {
    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
        config.corp_id, config.corp_secret
    );
    
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
    
    let result: AccessTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse response failed: {}", e))?;
    
    if result.errcode != 0 {
        return Err(format!("WeWork API error: {} - {}", result.errcode, result.errmsg));
    }
    
    result.access_token.ok_or_else(|| "No access token in response".to_string())
}

/// Get user info from code
async fn get_user_info(access_token: &str, code: &str) -> Result<UserInfoResponse, String> {
    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/auth/getuserinfo?access_token={}&code={}",
        access_token, code
    );
    
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
    
    let result: UserInfoResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse response failed: {}", e))?;
    
    if result.errcode != 0 {
        return Err(format!("WeWork API error: {} - {}", result.errcode, result.errmsg));
    }
    
    Ok(result)
}

/// Get user detail by userid
async fn get_user_detail(access_token: &str, userid: &str) -> Result<UserDetailResponse, String> {
    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/user/get?access_token={}&userid={}",
        access_token, userid
    );
    
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
    
    let result: UserDetailResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse response failed: {}", e))?;
    
    if result.errcode != 0 {
        return Err(format!("WeWork API error: {} - {}", result.errcode, result.errmsg));
    }
    
    Ok(result)
}
