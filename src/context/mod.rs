//! UpInfo 模块 - 请求上下文

mod context;
mod verify;
mod auth_middleware;

pub use context::{UpInfo, RequestBody};
pub use verify::{verify_sid_simple, verify_sid_db, verify_sid_web_db, get_lovers_state};
pub use auth_middleware::{AuthConfig, get_auth_config, sid_auth_middleware};

// 直接从 database 重导出
pub use database::{LoversDataState, VerifyResult};

// 保持向后兼容
pub type Context = UpInfo;
