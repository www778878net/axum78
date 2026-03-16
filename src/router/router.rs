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
    Router,
    routing::post,
    extract::{Path, State, Json as AxumJson},
    response::IntoResponse,
    http::StatusCode,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{ApiResponse, UpInfo, RequestBody};

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
}

/// 4级路由构建器
pub struct ApiRouter78 {
    controllers: HashMap<String, Arc<dyn Controller78>>,
}

impl ApiRouter78 {
    /// 创建新路由
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
        }
    }

    /// 注册控制器
    ///
    /// - path: api 路径，格式 "apisys/apimicro/apiobj"
    /// - controller: 控制器实现
    pub fn register<C: Controller78>(mut self, path: &str, controller: C) -> Self {
        self.controllers.insert(path.to_string(), Arc::new(controller));
        self
    }

    /// 构建最终路由
    pub fn build(self) -> Router {
        let state = Arc::new(RouterState {
            controllers: self.controllers,
        });

        Router::new()
            .route("/:apisys/:apimicro/:apiobj/:apifun", post(api_handler))
            .with_state(state)
    }
}

impl Default for ApiRouter78 {
    fn default() -> Self {
        Self::new()
    }
}

/// 4级路由 API 处理器
async fn api_handler(
    State(state): State<Arc<RouterState>>,
    Path((apisys, apimicro, apiobj, apifun)): Path<(String, String, String, String)>,
    AxumJson(body): AxumJson<RequestBody>,
) -> impl IntoResponse {
    // ========== 安全校验 ==========
    // apifun 不能以 "_" 开头 (私有方法)
    if apifun.starts_with('_') {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(ApiResponse::fail("Access denied: private method", -4)),
        );
    }

    // apisys 必须以 "api" 开头
    if !apisys.to_lowercase().starts_with("api") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(ApiResponse::fail("Access denied: invalid api system", -4)),
        );
    }

    // apimicro 不能以 "dll" 开头
    if apimicro.to_lowercase().starts_with("dll") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(ApiResponse::fail("Access denied: dll not allowed", -4)),
        );
    }

    // ========== 查找控制器 ==========
    let controller_path = format!("{}/{}/{}", apisys, apimicro, apiobj);
    let controller = match state.controllers.get(&controller_path) {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                axum::Json(ApiResponse::fail(&format!("Controller not found: {}", controller_path), -1)),
            );
        }
    };

    // ========== 构建 UpInfo ==========
    let mut up = UpInfo::from_route(&apisys, &apimicro, &apiobj, &apifun);
    up.fill_from_body(&body);

    // ========== 调用方法 ==========
    let result = controller.call(&mut up, &apifun).await;

    // ========== 构建响应 ==========
    if up.res != 0 {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiResponse::fail(&up.errmsg, up.res)),
        );
    }

    let mut resp = ApiResponse::success(result);
    resp.kind = up.backtype;
    (StatusCode::OK, axum::Json(resp))
}

/// 保留旧的 ApiRouter 别名 (向后兼容)
pub type ApiRouter = ApiRouter78;