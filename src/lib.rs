//! axum78 - 基于 axum 的快速 Web API 框架
//!
//! 参考 koa78-base78 的 4 级路由架构: /:apisys/:apimicro/:apiobj/:apifun
//!
//! # 示例
//!
//! ```rust,ignore
//! use axum78::{UpInfo, RequestBody, ApiResponse, ApiRouter78};
//! use serde_json::Value;
//!
//! // 1. 定义控制器
//! struct MyController;
//!
//! #[async_trait]
//! impl Controller78 for MyController {
//!     async fn call(&self, up: &mut UpInfo, fun: &str) -> Value {
//!         match fun {
//!             "get" => { /* ... */ }
//!             "mAdd" => { /* ... */ }
//!             _ => Value::Null
//!         }
//!     }
//! }
//!
//! // 2. 注册路由
//! let router = ApiRouter78::new()
//!     .register("apitest/testmenu/testtb", MyController)
//!     .build();
//! ```

pub mod context;
pub mod response;
pub mod base_api;
pub mod router;
pub mod server;
pub mod proto;
pub mod sync;
pub mod apigame;

// 重导出常用类型
pub use context::{UpInfo, RequestBody, Context};
pub use response::{ApiResponse, ApiError};
pub use base_api::{BaseApi, TableConfig};
pub use router::{ApiRouter, ApiRouter78, Controller78};
pub use server::Server;
pub use sync::{DataSync, SyncConfig, SyncResult};

// 重导出 async_trait
pub use async_trait::async_trait;

// 重导出 axum 常用类型
pub use axum::{
    Router,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};