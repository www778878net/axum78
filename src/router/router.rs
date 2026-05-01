//! API 路由 - 4级路由系统
//!
//! 4级路由: /:apisys/:apimicro/:apiobj/:apifun
//!
//! 安全校验:
//! - apisys 必须以 "api" 开头
//! - apifun 不能以 "_" 开头 (私有方法)
//! - apimicro 不能以 "dll" 开头

use axum::{
    body::Bytes,
    Router,
    routing::any,
    extract::Extension,
    response::IntoResponse,
    http::{header, StatusCode},
    middleware,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{ApiResponse, UpInfo, Response, sid_auth_middleware};
use tower_http::cors::{CorsLayer, Any};

/// 控制器 Trait - 实现此 trait 来定义 API 处理器
#[async_trait]
pub trait Controller78: Send + Sync + 'static {
    async fn call(&self, up: &mut UpInfo, fun: &str, method: &str) -> Value;
}

/// 路由状态 (内部共享)
#[derive(Clone)]
pub struct RouterState {
    pub controllers: HashMap<String, Arc<dyn Controller78>>,
    pub open_controllers: HashMap<String, Arc<dyn Controller78>>,
}

/// 4级路由构建器
pub struct ApiRouter78 {
    controllers: HashMap<String, Arc<dyn Controller78>>,
    open_controllers: HashMap<String, Arc<dyn Controller78>>,
}

impl ApiRouter78 {
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
            open_controllers: HashMap::new(),
        }
    }

    pub fn register<C: Controller78>(mut self, path: &str, controller: C) -> Self {
        self.controllers.insert(path.to_string(), Arc::new(controller));
        self
    }

    pub fn register_open<C: Controller78>(mut self, path: &str, controller: C) -> Self {
        self.open_controllers.insert(path.to_string(), Arc::new(controller));
        self
    }

    pub fn build(self) -> (HashMap<String, Arc<dyn Controller78>>, HashMap<String, Arc<dyn Controller78>>) {
        (self.controllers, self.open_controllers)
    }
}

/// 公开路由处理器 (无认证中间件)
async fn open_api_handler(
    Extension(state): Extension<Arc<RouterState>>,
    Extension(mut up): Extension<UpInfo>,
) -> impl IntoResponse {
    let apimicro_lower = up.apimicro.to_lowercase();
    let apifun_lower = up.apifun.to_lowercase();
    let method = up.method.clone();

    if up.apifun.starts_with('_') {
        return forbidden();
    }
    if apimicro_lower.starts_with("dll") {
        return forbidden();
    }

    let controller_path = format!("{}/{}", up.apimicro, up.apiobj);
    if let Some(controller) = state.open_controllers.get(&controller_path) {
        let result = controller.call(&mut up, &apifun_lower, &method).await;
        if up.res != 0 {
            return bad_request(up.errmsg, up.res);
        }
        let resp = ApiResponse::success(result);
        return (StatusCode::OK, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let resp = Response::fail(&format!("API not found: apiopen/{}/{}/{}", up.apimicro, up.apiobj, up.apifun), 404);
    (StatusCode::NOT_FOUND, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 4级路由处理器 (需要认证)
/// 中间件已经解析了所有信息到 up
async fn api_handler(
    Extension(state): Extension<Arc<RouterState>>,
    Extension(mut up): Extension<UpInfo>,
) -> impl IntoResponse {
    let apisys_lower = up.apisys.to_lowercase();
    let apimicro_lower = up.apimicro.to_lowercase();
    let apifun_lower = up.apifun.to_lowercase();
    let method = up.method.clone();

    if up.apifun.starts_with('_') {
        return forbidden();
    }
    if !apisys_lower.starts_with("api") {
        return forbidden();
    }
    if apimicro_lower.starts_with("dll") {
        return forbidden();
    }

    let controller_path = format!("{}/{}/{}", up.apisys, up.apimicro, up.apiobj);
    if let Some(controller) = state.controllers.get(&controller_path) {
        let result = controller.call(&mut up, &apifun_lower, &method).await;
        if up.res != 0 {
            return bad_request(up.errmsg, up.res);
        }
        let resp = ApiResponse::success(result);
        return (StatusCode::OK, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let resp = Response::fail(&format!("API not found: {}/{}/{}/{}", up.apisys, up.apimicro, up.apiobj, up.apifun), 404);
    (StatusCode::NOT_FOUND, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
}

/// 创建主路由器 (带认证中间件)
pub fn create_router() -> Router<()> {
    let (controllers, open_controllers) = ApiRouter78::new().build();
    let state = Arc::new(RouterState { controllers, open_controllers });

    build_router(state)
}

/// 创建主路由器 (带认证中间件)，允许注入自定义控制器
pub fn create_router_with_custom<F>(
    controller_injector: F,
) -> Router<()>
where
    F: FnOnce(ApiRouter78) -> (HashMap<String, Arc<dyn Controller78>>, HashMap<String, Arc<dyn Controller78>>),
{
    let (controllers, open_controllers) = controller_injector(ApiRouter78::new());
    let state = Arc::new(RouterState { controllers, open_controllers });

    build_router(state)
}

fn build_router(state: Arc<RouterState>) -> Router<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let open_router = Router::new()
        .route("/*api/*apimicro/*apiobj/*apifun", any(open_api_handler))
        .layer(Extension(state.clone()));

    let auth_router = Router::new()
        .route("/*apisys/*apimicro/*apiobj/*apifun", any(api_handler))
        .layer(Extension(state.clone()))
        .layer(middleware::from_fn(sid_auth_middleware));

    Router::new()
        .nest("/apiopen", open_router)
        .merge(auth_router)
        .layer(cors)
}

fn forbidden() -> (StatusCode, [(axum::http::HeaderName, &'static str); 1], Bytes) {
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/json")],
        Bytes::from(serde_json::to_string(&ApiResponse::fail("Access denied", -4)).unwrap_or_default()),
    )
}

fn bad_request(msg: String, res: i32) -> (StatusCode, [(axum::http::HeaderName, &'static str); 1], Bytes) {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/json")],
        Bytes::from(serde_json::to_string(&ApiResponse::fail(&msg, res)).unwrap_or_default()),
    )
}

fn content_type_json() -> [(axum::http::HeaderName, &'static str); 1] {
    [(header::CONTENT_TYPE, "application/json")]
}

/// 保留旧的 ApiRouter 别名 (向后兼容)
pub type ApiRouter = ApiRouter78;
