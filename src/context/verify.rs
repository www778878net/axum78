//! SID 验证模块
//!
//! 提供会话验证功能，使用 LoversDataState

use crate::context::LoversDataState;

/// 获取默认的 LoversDataState
pub fn get_lovers_state() -> LoversDataState {
    LoversDataState::new()
}
