//! SID 验证模块
//!
//! 提供会话验证功能，使用 LoversDataState

pub use database::{LoversDataState, VerifyResult};
use base::Response;

/// 获取默认的 LoversDataState
pub fn get_lovers_state() -> LoversDataState {
    LoversDataState::new()
}

/// 验证 SID
/// 
/// 查数据库验证 SID，返回用户信息（包括 CID）
pub fn verify_sid(sid: &str, lovers_state: &LoversDataState) -> Result<VerifyResult, Response> {
    if sid.is_empty() {
        return Err(Response::fail("无效的SID: sid为空", -1));
    }
    
    lovers_state.verify_sid(sid)
        .map_err(|e| Response::fail(&e, -1))
}

/// 验证 SID_web（Web会话模式）
pub fn verify_sid_web(sid_web: &str, lovers_state: &LoversDataState) -> Result<VerifyResult, Response> {
    if sid_web.is_empty() {
        return Err(Response::fail("无效的SID_web: sid_web为空", -1));
    }
    
    lovers_state.verify_sid_web(sid_web)
        .map_err(|e| Response::fail(&e, -1))
}
