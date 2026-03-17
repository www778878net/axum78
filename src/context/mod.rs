//! UpInfo 模块 - 请求上下文

mod context;
mod verify;
mod auth_middleware;
mod lovers_state;

pub use context::{UpInfo, RequestBody};
pub use verify::{verify_sid, get_lovers_state};
pub use auth_middleware::{AuthConfig, get_auth_config, sid_auth_middleware};
pub use lovers_state::{LoversDataState, VerifyResult, LOVERS_CREATE_SQL, LOVERS_AUTH_CREATE_SQL};

// 保持向后兼容
pub type Context = UpInfo;
