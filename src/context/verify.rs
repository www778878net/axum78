//! SID 验证模块
//!
//! 提供会话验证功能，使用 LoversDataState

pub use database::{LoversDataState, VerifyResult};
use base::Response;

/// 获取默认的 LoversDataState
pub fn get_lovers_state() -> LoversDataState {
    LoversDataState::new()
}

/// 验证 SID（简单模式）
/// 
/// 从 SID 中提取 CID，格式为 "cid" 或 "cid|other"
/// 不查数据库，仅做格式验证
pub fn verify_sid_simple(sid: &str) -> Result<VerifyResult, Response> {
    LoversDataState::verify_sid_simple(sid)
        .map_err(|e| Response::fail(&e, -1))
}

/// 验证 SID（数据库模式）
/// 
/// 从数据库验证 SID 是否有效
pub fn verify_sid_db(sid: &str, lovers_state: &LoversDataState) -> Result<VerifyResult, Response> {
    lovers_state.verify_sid(sid)
        .map_err(|e| Response::fail(&e, -1))
}

/// 验证 SID_web（Web会话模式）
pub fn verify_sid_web_db(sid_web: &str, lovers_state: &LoversDataState) -> Result<VerifyResult, Response> {
    lovers_state.verify_sid_web(sid_web)
        .map_err(|e| Response::fail(&e, -1))
}
