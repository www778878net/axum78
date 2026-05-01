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
    body::Bytes,
    Router,
    routing::any,
    extract::{Path, Extension, Request},
    response::IntoResponse,
    http::{header, StatusCode, Uri, Method},
    middleware,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{ApiResponse, UpInfo, RequestBody, Response, VerifyResult, sid_auth_middleware};
use tower_http::cors::{CorsLayer, Any};

#[derive(Clone, Debug)]
pub struct ApiPath {
    pub apisys: String,
    pub apimicro: String,
    pub apiobj: String,
    pub apifun: String,
}

/// 控制器 Trait - 实现此 trait 来定义 API 处理器
#[async_trait]
pub trait Controller78: Send + Sync + 'static {
    async fn call(&self, up: &mut UpInfo, fun: &str, method: &Method) -> Value;
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

    /// 消耗构建器，返回 controllers 和 open_controllers
    pub fn build(self) -> (HashMap<String, Arc<dyn Controller78>>, HashMap<String, Arc<dyn Controller78>>) {
        (self.controllers, self.open_controllers)
    }
}

/// 公开路由处理器 (无认证中间件)
async fn open_api_handler(
    Extension(state): Extension<Arc<RouterState>>,
    Path((apimicro, apiobj, apifun)): Path<(String, String, String)>,
    uri: Uri,
    method: Method,
    request: Request,
) -> impl IntoResponse {
    let apimicro_lower = apimicro.to_lowercase();
    let apifun_lower = apifun.to_lowercase();

    if apifun.starts_with('_') {
        return forbidden();
    }
    if apimicro_lower.starts_with("dll") {
        return forbidden();
    }

    let body_bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024).await.unwrap_or_default();

    let controller_path = format!("{}/{}", apimicro, apiobj);
    if let Some(controller) = state.open_controllers.get(&controller_path) {
        let mut up = UpInfo::default();
        up.apisys = "apiopen".to_string();
        up.apimicro = apimicro.clone();
        up.apiobj = apiobj.clone();
        up.apifun = apifun.clone();
        if !body_bytes.is_empty() {
            if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
                up.jsdata = Some(body_str.to_string());
            }
        }
        if let Some(query_str) = uri.query() {
            up.source = query_str.to_string();
        }

        let result = controller.call(&mut up, &apifun_lower, &method).await;
        if up.res != 0 {
            return bad_request(up.errmsg, up.res);
        }
        let resp = ApiResponse::success(result);
        return (StatusCode::OK, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    match (apimicro_lower.as_str(), apiobj.as_str()) {
        ("wework", "auth") => {
            return crate::apiopen::wework::auth::handle_raw(&apifun_lower, body_bytes).await;
        }
        _ => {
            let resp = Response::fail(&format!("API not found: apiopen/{}/{}/{}", apimicro, apiobj, apifun), 404);
            return (StatusCode::NOT_FOUND, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    }
}

/// 4级路由处理器 (需要认证)
async fn api_handler(
    Extension(state): Extension<Arc<RouterState>>,
) -> impl IntoResponse {
    let method = axum::http::Method::GET;
    let (apisys, apimicro, apiobj, apifun) = (String::new(), String::new(), String::new(), String::new());
    let verify_result = VerifyResult::new("", "", "guest");
    let mut up = UpInfo::default();
    up.cid = verify_result.cid.clone();
    up.uid = verify_result.uid.clone();
    up.uname = verify_result.uname.clone();

    if apifun.starts_with('_') {
        return forbidden();
    }
    if !apisys.to_lowercase().starts_with("api") {
        return forbidden();
    }
    if apimicro.to_lowercase().starts_with("dll") {
        return forbidden();
    }

    let controller_path = format!("{}/{}/{}", apisys, apimicro, apiobj);
    if let Some(controller) = state.controllers.get(&controller_path) {
        let result = controller.call(&mut up, &apifun, &method).await;
        if up.res != 0 {
            return bad_request(up.errmsg, up.res);
        }
        let resp = ApiResponse::success(result);
        return (StatusCode::OK, content_type_json(), Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
    }

    let (status, resp_bytes) = match (apisys.to_lowercase().as_str(), apimicro.to_lowercase().as_str(), apiobj.as_str()) {
        ("apitest", "testmenu", "testtb") => crate::apitest::testmenu::testtb::handle(&apifun, up, &verify_result).await,
        ("apisvc", "backsvc", "synclog") => crate::apisvc::backsvc::synclog::handle(&apifun, up).await,
        ("apisvc", "backsvc", "synclog_mysql") => crate::apisvc::backsvc::synclog_mysql::handle(&apifun, up, &verify_result).await,
        ("apigame", "mock", "game_state") => crate::apigame::mock::game_state::handle(&apifun, up).await,
        _ => {
            let resp = Response::fail(&format!("API not found: {}/{}/{}/{}", apisys, apimicro, apiobj, apifun), 404);
            (StatusCode::NOT_FOUND, Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    };
    (status, content_type_json(), resp_bytes)
}

/// 创建主路由器 (带认证中间件)
pub fn create_router() -> Router<()> {
    let (controllers, open_controllers) = ApiRouter78::new().build();
    let state = Arc::new(RouterState { controllers, open_controllers });

    build_router(state)
}

/// 创建主路由器 (带认证中间件)，允许注入自定义控制器
pub fn create_router_with_custom<F, G>(
    controller_injector: F,
    customize_open_router: G,
) -> Router<()>
where
    F: FnOnce(ApiRouter78) -> (HashMap<String, Arc<dyn Controller78>>, HashMap<String, Arc<dyn Controller78>>),
    G: FnOnce(Router<()>) -> Router<()>,
{
    let (controllers, open_controllers) = controller_injector(ApiRouter78::new());
    let state = Arc::new(RouterState { controllers, open_controllers });

    build_router_with_custom(state, customize_open_router)
}

fn build_router(state: Arc<RouterState>) -> Router<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let open_router = Router::new()
        .route("/:apimicro/:apiobj/:apifun", any(open_api_handler))
        .layer(Extension(state.clone()));

    let auth_router = Router::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
        .layer(Extension(state.clone()))
        .layer(middleware::from_fn(sid_auth_middleware));

    Router::new()
        .nest("/apiopen", open_router)
        .merge(auth_router)
        .layer(cors)
}

fn build_router_with_custom<G>(
    state: Arc<RouterState>,
    customize_open_router: G,
) -> Router<()>
where
    G: FnOnce(Router<()>) -> Router<()>,
{
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let open_router_base = Router::new();
    let open_router_customized = customize_open_router(open_router_base);
    let open_router = open_router_customized
        .route("/:apimicro/:apiobj/:apifun", any(open_api_handler))
        .layer(Extension(state.clone()));

    let auth_router = Router::new()
        .route("/:apisys/:apimicro/:apiobj/:apifun", any(api_handler))
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
