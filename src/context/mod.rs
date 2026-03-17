//! UpInfo 模块 - 请求上下文

mod context;
mod verify;

pub use context::{UpInfo, RequestBody};
pub use verify::{verify_sid_simple, verify_sid_db, verify_sid_web_db, get_lovers_state};

// 直接从 database 重导出
pub use database::{LoversDataState, VerifyResult};

// 保持向后兼容
pub type Context = UpInfo;
