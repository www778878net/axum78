//! API 路由 - 4级路由系统
//!
//! 参考 koa78-base78 的 4 级路由:
//! /:apisys/:apimicro/:apiobj/:apifun
//!
//! 安全校验:
//! - apisys 必须以 "api" 开头
//! - apifun 不能以 "_" 开头 (私有方法)
//! - apimicro 不能以 "dll" 开头

use axum::{
    body::{Body, Bytes},
    Router,
    routing::any,
    extract::{Path, State, Json as AxumJson, Query, Request},
    response::IntoResponse,
    http::{header, StatusCode, Uri, Method},
    middleware,
    Extension,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{ApiResponse, UpInfo, RequestBody, Response, VerifyResult, sid_auth_middleware};
use tower_http::cors::{CorsLayer, Any};

/// 控制器 Trait - 实现此 trait 来定义 API 处理器
#[async_trait]
pub trait Controller78: Send + Sync + 'static {
    /// 调用指定方法
    ///
    /// - up: 请求上下文 (已填充路由参数和请求体)
    /// - fun: 方法名 (apifun)
    /// - 返回: 响应数据
    async fn call(&self, up: &mut UpInfo, fun: &str) -> Value;
}

/// 路由状态 (内部共享)
struct RouterState {
    controllers: HashMap<String, Arc<dyn Controller78>>,
    open_controllers: HashMap<String, Arc<dyn Controller78>>,
}

/// 4级路由构建器
pub struct ApiRouter78 {
    /// 需要认证的路由
    controllers: HashMap<String, Arc<dyn Controller78>>,
    /// 公开路由 (无中间件)
    open_controllers: HashMap<String, Arc<dyn Controller78>>,
}

impl ApiRouter78 {
    /// 创建新路由
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
            open_controllers: HashMap::new(),
        }
    }

    /// 注册控制器 (需要认证)
    ///
    /// - path: api 路径，格式 "apisys/apimicro/apiobj"
    /// - controller: 控制器实现
    pub fn register<C: Controller78>(mut self, path: &str, controller: C) -> Self {
        self.controllers.insert(path.to_string(), Arc::new(controller));
        self
    }

    /// 注册公开控制器 (无认证中间件)
    ///
    /// - path: api 路径，格式 "apimicro/apiobj" (不需要 apisys 前缀)
    /// - controller: 控制器实现
    /// - 路由会挂载到 /apiopen/:apimicro/:apiobj/:apifun
    pub fn register_open<C: Controller78>(mut self, path: &str, controller: C) -> Self {
        self.open_controllers.insert(path.to_string(), Arc::new(controller));
        self
    }

    /// 构建最终路由（带CORS和认证中间件）
    pub fn build(self) -> Router {
        let state = Arc::new(RouterState {
            controllers: self.controllers,
            open_controllers: self.open_controllers,
        });

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

        // 公开路由 - 无中间件
        let open_router = Router::new()
            .route("/:apimicro/:apiobj/:apifun", any(open_api_handler))
            .with_state(state.clone());

        // 认证路由 - 有中间件
        let auth_router = Router::new()
            .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
            .layer(middleware::from_fn(sid_auth_middleware))
            .with_state(state);

        Router::new()
            .nest("/apiopen", open_router)
            .merge(auth_router)
            .layer(cors)
    }
}

impl Default for ApiRouter78 {
    fn default() -> Self {
        Self::new()
    }
}

/// 公开路由 API 处理器 (无认证中间件)
async fn open_api_handler(
    State(state): State<Arc<RouterState>>,
    Path((apimicro, apiobj, apifun)): Path<(String, String, String)>,
    uri: Uri,
    method: Method,
    request: Request,
) -> impl IntoResponse {
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    // 安全校验: apifun 不能以 "_" 开头 (私有方法)
    if apifun.starts_with('_') {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied: private method", -4)).unwrap_or_default()),
        );
    }

    // apimicro 不能以 "dll" 开头
    if apimicro_lower.starts_with("dll") {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied: dll not allowed", -4)).unwrap_or_default()),
        );
    }

    // 查找控制器
    let controller_path = format!("{}/{}", apimicro, apiobj);
    let controller = match state.open_controllers.get(&controller_path) {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Bytes::from(serde_json::to_string(&ApiResponse::fail(&format!("Controller not found: {}", controller_path), -1)).unwrap_or_default()),
            );
        }
    };

    // 创建 UpInfo (无认证信息)
    let mut up = UpInfo::default();
    up.apisys = "apiopen".to_string();
    up.apimicro = apimicro.clone();
    up.apiobj = apiobj.clone();
    up.apifun = apifun.clone();

    // 解析请求体 -> jsdata
    let body_bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    
    if !body_bytes.is_empty() {
        if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
            up.jsdata = Some(body_str.to_string());
        }
    }

    // query 参数暂存到 source 字段 (简化处理)
    if let Some(query_str) = uri.query() {
        up.source = query_str.to_string();
    }

    // 调用方法
    let result = controller.call(&mut up, &apifun_lower).await;

    // 构建响应
    if up.res != 0 {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail(&up.errmsg, up.res)).unwrap_or_default()),
        );
    }

    let resp = ApiResponse::success(result);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Bytes::from(serde_json::to_string(&resp).unwrap_or_default()),
    )
}

/// 4级路由 API 处理器
async fn api_handler(
    State(state): State<Arc<RouterState>>,
    Extension((apisys, apimicro, apiobj, apifun)): Extension<(String, String, String, String)>,
    Extension(verify_result): Extension<VerifyResult>,
    Extension(mut up): Extension<UpInfo>,
) -> impl IntoResponse {
    // ========== 填充UpInfo ==========
    up.cid = verify_result.cid.clone();
    up.uid = verify_result.uid.clone();
    up.uname = verify_result.uname.clone();

    // ========== 安全校验 ==========
    // apifun 不能以 "_" 开头 (私有方法)
    if apifun.starts_with('_') {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied: private method", -4)).unwrap_or_default()),
        );
    }

    // apisys 必须以 "api" 开头
    if !apisys.to_lowercase().starts_with("api") {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied: invalid api system", -4)).unwrap_or_default()),
        );
    }

    // apimicro 不能以 "dll" 开头
    if apimicro.to_lowercase().starts_with("dll") {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied: dll not allowed", -4)).unwrap_or_default()),
        );
    }

    // ========== 查找控制器 ==========
    let controller_path = format!("{}/{}/{}", apisys, apimicro, apiobj);
    let controller = match state.controllers.get(&controller_path) {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                Bytes::from(serde_json::to_string(&ApiResponse::fail(&format!("Controller not found: {}", controller_path), -1)).unwrap_or_default()),
            );
        }
    };

    // ========== 调用方法 ==========
    let result = controller.call(&mut up, &apifun).await;

    // ========== 构建响应 ==========
    if up.res != 0 {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            Bytes::from(serde_json::to_string(&ApiResponse::fail(&up.errmsg, up.res)).unwrap_or_default()),
        );
    }

    let resp = ApiResponse::success(result);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Bytes::from(serde_json::to_string(&resp).unwrap_or_default()),
    )
}

/// Root API handler for 4-level routes with middleware
async fn root_api_handler(
    Extension((apisys, apimicro, apiobj, apifun)): Extension<(String, String, String, String)>,
    Extension(verify_result): Extension<VerifyResult>,
    Extension(up): Extension<UpInfo>,
) -> impl IntoResponse {
    let apisys_lower = apisys.to_lowercase();
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    let (status, resp_bytes) = match (apisys_lower.as_str(), apimicro_lower.as_str(), apiobj.as_str()) {
        ("apitest", "testmenu", "testtb") => {
            crate::apitest::testmenu::testtb::handle(&apifun_lower, up, &verify_result).await
        }
        ("apisvc", "backsvc", "synclog") => {
            crate::apisvc::backsvc::synclog::handle(&apifun_lower, up).await
        }
        ("apisvc", "backsvc", "synclog_mysql") => {
            crate::apisvc::backsvc::synclog_mysql::handle(&apifun_lower, up, &verify_result).await
        }
        ("apigame", "mock", "game_state") => {
            crate::apigame::mock::game_state::handle(&apifun_lower, up).await
        }
        ("apiopen", "wework", "auth") => {
            crate::apiopen::wework::auth::handle(&apifun_lower, up).await
        }
        _ => {
            let resp = Response::fail(&format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    };

    (status, [(header::CONTENT_TYPE, "application/json")], resp_bytes)
}

/// 创建主路由器 (带认证中间件)
pub fn create_router() -> Router<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    
    // apiopen routes - 不走中间件
    let apiopen_router = Router::new()
        .route("/:apimicro/:apiobj/:apifun", any(apiopen_handler));
    
    // api routes - 走中间件
    let api_router = Router::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(root_api_handler))
        .layer(middleware::from_fn(sid_auth_middleware));
    
    Router::new()
        .nest("/apiopen", apiopen_router)
        .merge(api_router)
        .layer(cors)
}

/// apiopen 处理器 - 不走中间件，直接处理
async fn apiopen_handler(
    Path((apimicro, apiobj, apifun)): Path<(String, String, String)>,
    uri: Uri,
    method: Method,
    request: Request,
) -> impl IntoResponse {
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();
    
    tracing::info!("apiopen: {}/{}/{} {}", apimicro, apiobj, apifun, method);
    
    // Collect query params
    let query: std::collections::HashMap<String, String> = uri
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default();
    
    // Collect body
    let body_bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    
    match (apimicro_lower.as_str(), apiobj.as_str()) {
        ("wework", "callback") => {
            crate::apiopen::wework::callback::handle_raw(&apifun_lower, &method, &query, body_bytes).await
        }
        ("wework", "auth") => {
            crate::apiopen::wework::auth::handle_raw(&apifun_lower, body_bytes).await
        }
        _ => {
            let resp = Response::fail(&format!("API not found: apiopen/{}/{}/{}", apimicro, apiobj, apifun), 404);
            (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// 保留旧的 ApiRouter 别名 (向后兼容)
pub type ApiRouter = ApiRouter78;
