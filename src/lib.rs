//! axum78 - 基于 axum 的快速 Web API 框架
//!
//! 目标：通过实现 BaseApi trait 快速定义表的 CRUD API
//!
//! # 示例
//!
//! ```rust,ignore
//! use axum78::{BaseApi, ApiRouter, TableConfig, Context, ApiError};
//! use serde::{Deserialize, Serialize};
//! use database::Sqlite78;
//! use async_trait::async_trait;
//!
//! // 1. 定义表实体
//! #[derive(Serialize, Deserialize, Clone)]
//! pub struct Testtb {
//!     pub kind: String,
//!     pub item: String,
//!     pub data: String,
//! }
//!
//! // 2. 实现 BaseApi trait
//! pub struct TesttbApi {
//!     db: Sqlite78,
//!     ctx: Context,
//! }
//!
//! #[async_trait]
//! impl BaseApi for TesttbApi {
//!     type Entity = Testtb;
//!
//!     fn config(&self) -> &TableConfig {
//!         static CONFIG: TableConfig = TableConfig {
//!             tbname: "testtb",
//!             id_field: "id",
//!             idpk_field: "idpk",
//!             uidcid: "cid",
//!             cols: vec!["kind".into(), "item".into(), "data".into()],
//!         };
//!         &CONFIG
//!     }
//!
//!     fn db(&self) -> &Sqlite78 { &self.db }
//!     fn context(&self) -> &Context { &self.ctx }
//! }
//!
//! // 3. 注册路由
//! let router = ApiRouter::new().register(TesttbApi { db, ctx });
//! // 自动生成路由: GET/POST /api/testtb, GET/PUT/DELETE /api/testtb/{id}
//! ```

pub mod context;
pub mod response;
pub mod base_api;
pub mod router;
pub mod server;
pub mod proto;
pub mod sync;

// 重导出常用类型
pub use context::Context;
pub use response::{ApiResponse, ApiError};
pub use base_api::{BaseApi, TableConfig};
pub use router::ApiRouter;
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