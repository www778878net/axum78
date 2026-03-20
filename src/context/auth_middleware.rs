//! SID 验证中间件
//!
//! 提供路由层统一验证功能

use axum::{
    body::Bytes,
    http::{header, StatusCode, Request},
    extract::Path as AxumPath,
    response::{IntoResponse, Response},
    middleware::Next,
};
use base::{UpInfo, Response as BaseResponse, ProjectPath, MyLogger};
use crate::{get_lovers_state, LoversDataState, VerifyResult};
use std::collections::HashSet;
use serde::{Serialize, Deserialize};

static LOGGER: std::sync::OnceLock<MyLogger> = std::sync::OnceLock::new();

fn get_logger() -> &'static MyLogger {
    LOGGER.get_or_init(|| MyLogger::new("auth_middleware", 7))
}

/// 认证配置
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// 一级白名单：apisys（如 apiguest）
    pub skip_apisys: HashSet<String>,
    /// 二级白名单：apisys/apimicro（如 apitest/test）
    pub skip_apimicro: HashSet<String>,
    /// 三级白名单：apisys/apimicro/apiobj（如 apiuser/user/login）
    pub skip_routes: HashSet<String>,
}

impl AuthConfig {
    /// 从配置文件加载
    pub fn load() -> Self {
        let mut config = Self::default();
        
        if let Ok(project_path) = ProjectPath::find() {
            if let Ok(ini_config) = project_path.load_ini_config() {
                if let Some(auth_section) = ini_config.get("auth") {
                    if let Some(skip_apisys) = auth_section.get("skip_apisys") {
                        config.skip_apisys = skip_apisys
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    
                    if let Some(skip_apimicro) = auth_section.get("skip_apimicro") {
                        config.skip_apimicro = skip_apimicro
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    
                    if let Some(skip_routes) = auth_section.get("skip_routes") {
                        config.skip_routes = skip_routes
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
            }
        }
        
        config
    }
    
    /// 检查是否跳过验证
    pub fn should_skip(&self, apisys: &str, apimicro: &str, apiobj: &str) -> bool {
        let apisys_lower = apisys.to_lowercase();
        let apimicro_lower = apimicro.to_lowercase();
        let apiobj_lower = apiobj.to_lowercase();
        
        if self.skip_apisys.contains(&apisys_lower) {
            return true;
        }
        
        let level2 = format!("{}/{}", apisys_lower, apimicro_lower);
        if self.skip_apimicro.contains(&level2) {
            return true;
        }
        
        let level3 = format!("{}/{}/{}", apisys_lower, apimicro_lower, apiobj_lower);
        if self.skip_routes.contains(&level3) {
            return true;
        }
        
        false
    }
}

static AUTH_CONFIG: std::sync::OnceLock<AuthConfig> = std::sync::OnceLock::new();

/// 获取认证配置
pub fn get_auth_config() -> &'static AuthConfig {
    AUTH_CONFIG.get_or_init(AuthConfig::load)
}

/// 最小请求体（用于解析客户端请求）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct MinimalRequest {
    #[serde(default)]
    pub sid: String,
    #[serde(default)]
    pub cid: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub getstart: i32,
    #[serde(default)]
    pub getnumber: i32,
    #[serde(default)]
    pub order: String,
    #[serde(default)]
    pub bcid: String,
    #[serde(default)]
    pub mid: String,
    #[serde(default)]
    pub jsdata: Option<String>,
    #[serde(default)]
    pub bytedata: Option<Vec<u8>>,
}

impl Default for MinimalRequest {
    fn default() -> Self {
        Self {
            sid: String::new(),
            cid: String::new(),
            uid: String::new(),
            uname: "guest".to_string(),
            getstart: 0,
            getnumber: 15,
            order: "idpk desc".to_string(),
            bcid: String::new(),
            mid: String::new(),
            jsdata: None,
            bytedata: None,
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
        up.getstart = min.getstart;
        up.getnumber = min.getnumber;
        up.order = min.order;
        up.bcid = min.bcid;
        up.mid = min.mid;
        up.jsdata = min.jsdata;
        up.bytedata = min.bytedata;
        up
    }
}

/// SID 验证中间件（路由层）
/// 
/// 白名单规则（三级）：
/// - 一级：apisys（如 apiguest）
/// - 二级：apisys/apimicro（如 apitest/test）
/// - 三级：apisys/apimicro/apiobj（如 apiuser/user/login）
pub async fn sid_auth_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let logger = get_logger();
    let auth_config = get_auth_config();
    
    // 从URI中解析路径参数
    let path = request.uri().path();
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    
 
    
    let (apisys, apimicro, apiobj, apifun) = match parts.as_slice() {
        [apisys, apimicro, apiobj, apifun] => (apisys.to_string(), apimicro.to_string(), apiobj.to_string(), apifun.to_string()),
        _ => {
            let resp = BaseResponse::fail(&format!("Invalid path: {}", path), 400);
            return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default())).into_response();
        }
    };
    
    let apisys_lower = apisys.to_lowercase();
    let apimicro_lower = apimicro.to_lowercase();
    let apiobj_lower = apiobj.to_lowercase();
    
    logger.detail(&format!("sid_auth_middleware: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun));
    
    // 基础访问控制
    if apifun.starts_with('_') || !apisys_lower.starts_with("api") || apimicro_lower.starts_with("dll") {
        logger.error(&format!("Access denied: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun));
        let resp = BaseResponse::fail("Access denied", 403);
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default())).into_response();
    }

    // 保存URI，因为into_body()会消费request
    let uri = request.uri().clone();
    
    // 解析请求体
    let body_bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    
    // 使用最小请求体解析，然后转换为UpInfo
    let up: UpInfo = match serde_json::from_slice::<MinimalRequest>(&body_bytes) {
        Ok(min) => min.into(),
        Err(e) => {
            // For wework/callback routes, allow empty body (GET request or XML body)
            let is_wework_callback = apisys_lower == "apimes" 
                && apimicro_lower == "wework" 
                && apiobj_lower == "callback";
            
            if is_wework_callback || body_bytes.is_empty() {
                logger.detail(&format!("跳过JSON解析: {}/{}/{}", apisys_lower, apimicro_lower, apiobj_lower));
                UpInfo::new()
            } else {
                logger.error(&format!("解析请求失败: {}", e));
                let resp = BaseResponse::fail(&format!("解析请求失败: {}", e), -1);
                return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default())).into_response();
            }
        }
    };

    // 检查白名单
    if auth_config.should_skip(&apisys_lower, &apimicro_lower, &apiobj_lower) {
        logger.detail(&format!("跳过认证: {}/{}/{}", apisys_lower, apimicro_lower, apiobj_lower));
        let verify_result = VerifyResult::new(&up.cid, &up.uid, &up.uname);
        let mut builder = Request::new(axum::body::Body::from(body_bytes));
        *builder.uri_mut() = uri;
        builder.extensions_mut().insert(verify_result);
        builder.extensions_mut().insert(up);
        builder.extensions_mut().insert((apisys, apimicro, apiobj, apifun));
        return next.run(builder).await;
    }

    
    let lovers_state = get_lovers_state();
    let verify_result = match lovers_state.verify_sid(&up.sid) {
        Ok(v) => v,
        Err(e) => {
            logger.detail(&format!("验证失败，使用GUEST身份: {}", e));
            VerifyResult::new("GUEST000-8888-8888-8888-GUEST00GUEST", "", "guest")
        }
    };

    
    let mut builder = Request::new(axum::body::Body::from(body_bytes));
    *builder.uri_mut() = uri;
    builder.extensions_mut().insert(verify_result);
    builder.extensions_mut().insert(up);
    builder.extensions_mut().insert((apisys, apimicro, apiobj, apifun));
    
    next.run(builder).await
}
