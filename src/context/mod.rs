//! UpInfo 模块 - 请求上下文

pub mod auth_middleware;
pub mod context;
pub mod verify;
pub mod lovers_state;

pub use context::{UpInfo, RequestBody};
pub use verify::get_lovers_state;
pub use auth_middleware::{AuthConfig, get_auth_config, sid_auth_middleware};
pub use lovers_state::{
    LoversDataState, LoversDataStateMysql, VerifyResult, UserInfo,
    LOVERS_CREATE_SQL, LOVERS_AUTH_CREATE_SQL,
};

// 保持向后兼容
pub type Context = UpInfo;
