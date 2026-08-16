//! 全局 Controller 注册表
//!
//! 模仿 koa78base 的 ControllerLoader，所有 handler 按 `apisys/apimicro/apiobj` 路径注册，
//! api_handler 查表转发，无需手动 match 分支。
//!
//! 两张全局表：
//! 1. `REGISTRY`（Controller 表，走四级 /:apisys/:apimicro/:apiobj/:apifun）
//! 2. `ROUTE_REGISTRY`（裸路由表，非四级结构，如 /healthz、/chat/stream、静态页等）
//!
//! 用法：
//!   registry::register("apisvc/backsvc/datasync", Arc::new(DatasyncController));
//!   let ctrl = registry::lookup("apisvc/backsvc/datasync");
//!
//!   registry::register_route(Method::GET, "/healthz", handler);

use crate::router::Controller78;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::Router;

type ControllerMap = HashMap<String, Arc<dyn Controller78>>;

static REGISTRY: Lazy<RwLock<ControllerMap>> = Lazy::new(|| RwLock::new(HashMap::new()));

/// 注册 controller
pub fn register(path: &str, controller: Arc<dyn Controller78>) {
    REGISTRY.write().unwrap().insert(path.to_string(), controller);
}

/// 查找 controller（大小写不敏感：先精确匹配，再小写匹配）
pub fn lookup(path: &str) -> Option<Arc<dyn Controller78>> {
    let reg = REGISTRY.read().unwrap();
    reg.get(path)
        .cloned()
        .or_else(|| reg.get(&path.to_lowercase()).cloned())
}

// ============ 裸路由注册表（非四级结构端点） ============

/// 裸路由条目：注册时直接构造 `Router<()>` 片段，启动时 merge 进主 Router。
/// 类型安全（各 handler 可返回 Html/Sse/Json 等），无需 Box<dyn>。
type RouteEntry = (axum::http::Method, String, Router<()>);

static ROUTE_REGISTRY: Lazy<RwLock<Vec<RouteEntry>>> = Lazy::new(|| RwLock::new(Vec::new()));

/// 注册裸路由（healthz / SSE / 静态页 / mysql-config 等非四级结构端点）
///
/// 用法：
///   registry::register_route(Method::GET, "/healthz", Router::new().route("/healthz", get(handler)));
///
/// 说明：`router` 参数是一个仅含该路径一条 route 的 `Router<()>` 片段，
/// 启动时会被 `build_router` 逐个 merge 进主 Router（精确路由优先于通配路由）。
pub fn register_route(method: axum::http::Method, path: &str, router: Router<()>) {
    ROUTE_REGISTRY
        .write()
        .unwrap()
        .push((method, path.to_string(), router));
}

/// 取出所有已注册的裸路由片段（供 build_router 调用）
pub fn take_routes() -> Vec<Router<()>> {
    ROUTE_REGISTRY
        .read()
        .unwrap()
        .iter()
        .map(|(_, _, r)| r.clone())
        .collect()
}
