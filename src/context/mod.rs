//! UpInfo 模块 - 请求上下文

mod context;
pub use context::{UpInfo, RequestBody};

// 保持向后兼容
pub type Context = UpInfo;