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
            logger.error(&format!("Invalid path: {}", path));
            let resp = BaseResponse::fail("Invalid path", 400);
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
    
    let up: UpInfo = match serde_json::from_slice(&body_bytes) {
        Ok(u) => u,
        Err(e) => {
            logger.error(&format!("解析请求失败: {}", e));
            let resp = BaseResponse::fail(&format!("解析请求失败: {}", e), -1);
            return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default())).into_response();
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
        return next.run(builder).await;
    }

    
    let lovers_state = get_lovers_state();
    let verify_result = match lovers_state.verify_sid(&up.sid) {
        Ok(v) => v,
        Err(e) => {
            logger.error(&format!("验证失败: {}", e));
            let resp = BaseResponse::fail(&e, -1);
            return (StatusCode::UNAUTHORIZED, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default())).into_response();
        }
    };

    
    let mut builder = Request::new(axum::body::Body::from(body_bytes));
    *builder.uri_mut() = uri;
    builder.extensions_mut().insert(verify_result);
    builder.extensions_mut().insert(up);
    
    next.run(builder).await
}
