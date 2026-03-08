//! API 路由 - 自动生成 CRUD 路由
//!
//! 根据 BaseApi trait 自动生成:
//! - GET    /api/{table}      → get (查询)
//! - GET    /api/{table}/:id  → get_by_id (单条)
//! - POST   /api/{table}      → m_add (新增)
//! - PUT    /api/{table}/:id  → m_update (更新)
//! - DELETE /api/{table}/:id  → m_del (删除)

use axum::{
    Router,
    routing::{get, post, put, delete},
    extract::{Path, Query},
    Json,
};
use serde_json::Value;
use std::collections::HashMap;

use crate::{ApiResponse, BaseApi};

/// API 路由构建器
pub struct ApiRouter {
    router: Router<()>,
}

impl ApiRouter {
    /// 创建新路由
    pub fn new() -> Self {
        Self {
            router: Router::new(),
        }
    }

    /// 注册一个表 API (自动生成 CRUD 路由)
    pub fn register<T: BaseApi + Clone + Send + Sync + 'static>(mut self, api: T) -> Self {
        let table = api.config().tbname.clone();

        // GET /api/{table} - 查询列表
        let api_clone = api.clone();
        let route = format!("/api/{}", table);
        self.router = self.router.route(
            &route,
            get(move |Query(params): Query<HashMap<String, String>>| {
                let api = api_clone.clone();
                async move {
                    let params: HashMap<String, Value> = params
                        .into_iter()
                        .map(|(k, v)| (k, Value::String(v)))
                        .collect();

                    match api.get(params).await {
                        Ok(data) => Json(ApiResponse::success(serde_json::to_value(data).unwrap_or(Value::Null))),
                        Err(e) => Json(ApiResponse::fail(&e.message, e.code)),
                    }
                }
            }),
        );

        // GET /api/{table}/:id - 查询单条
        let api_clone = api.clone();
        let route = format!("/api/{}/:id", table);
        self.router = self.router.route(
            &route,
            get(move |Path(id): Path<String>| {
                let api = api_clone.clone();
                async move {
                    match api.get_by_id(&id).await {
                        Ok(Some(data)) => Json(ApiResponse::success(serde_json::to_value(data).unwrap_or(Value::Null))),
                        Ok(None) => Json(ApiResponse::fail("记录不存在", -1)),
                        Err(e) => Json(ApiResponse::fail(&e.message, e.code)),
                    }
                }
            }),
        );

        // POST /api/{table} - 新增
        let api_clone = api.clone();
        let route = format!("/api/{}", table);
        self.router = self.router.route(
            &route,
            post(move |Json(entity): Json<T::Entity>| {
                let api = api_clone.clone();
                async move {
                    match api.m_add(&entity).await {
                        Ok(id) => Json(ApiResponse::success(Value::String(id))),
                        Err(e) => Json(ApiResponse::fail(&e.message, e.code)),
                    }
                }
            }),
        );

        // PUT /api/{table}/:id - 更新
        let api_clone = api.clone();
        let route = format!("/api/{}/:id", table);
        self.router = self.router.route(
            &route,
            put(move |Path(id): Path<String>, Json(entity): Json<T::Entity>| {
                let api = api_clone.clone();
                async move {
                    match api.m_update(&id, &entity).await {
                        Ok(true) => Json(ApiResponse::success(Value::Bool(true))),
                        Ok(false) => Json(ApiResponse::fail("记录不存在或无更新", -1)),
                        Err(e) => Json(ApiResponse::fail(&e.message, e.code)),
                    }
                }
            }),
        );

        // DELETE /api/{table}/:id - 删除
        let api_clone = api;
        let route = format!("/api/{}/:id", table);
        self.router = self.router.route(
            &route,
            delete(move |Path(id): Path<String>| {
                let api = api_clone.clone();
                async move {
                    match api.m_del(&id).await {
                        Ok(true) => Json(ApiResponse::success(Value::String(id))),
                        Ok(false) => Json(ApiResponse::fail("记录不存在", -1)),
                        Err(e) => Json(ApiResponse::fail(&e.message, e.code)),
                    }
                }
            }),
        );

        self
    }

    /// 构建最终路由
    pub fn build(self) -> Router<()> {
        self.router
    }
}

impl Default for ApiRouter {
    fn default() -> Self {
        Self::new()
    }
}