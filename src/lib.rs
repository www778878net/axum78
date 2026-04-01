//! axum78 - 基于 axum 的快速 Web API 框架
//!
//! 参考 koa78-base78 的 4 级路由架构: /:apisys/:apimicro/:apiobj/:apifun
//!
//! 请求格式: UpInfo (JSON)
//! 响应格式: Response (JSON)

pub mod base78;
pub mod context;
pub mod response;
pub mod base_api;
pub mod router;
pub mod server;

pub mod apitest;
pub mod apisvc;
pub mod apigame;
pub mod apiopen;

pub use base78::{Base78, CidBase78};
pub use context::{UpInfo, RequestBody, Context, VerifyResult, get_lovers_state, LoversDataState, LoversDataStateMysql, UserInfo, LOVERS_CREATE_SQL, LOVERS_AUTH_CREATE_SQL, AuthConfig, get_auth_config, sid_auth_middleware};
pub use response::{ApiResponse, ApiError};
pub use base_api::{BaseApi, TableConfig};
pub use router::{ApiRouter78, Controller78, create_router, create_router_with_custom};
pub use server::Server;

pub use async_trait::async_trait;

pub use axum::{
    Router,
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

pub use base::Response;

// Re-export wework config
pub use apiopen::wework::get_wework_config;

// Re-export database types for convenience
pub use datastate::{Mysql78, MysqlConfig, MysqlUpInfo, next_id_string};
