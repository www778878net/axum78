//! SID 验证模块
//!
//! 提供会话验证功能，使用 LoversDataState

use crate::context::LoversDataState;

/// 获取默认的 LoversDataState
pub fn get_lovers_state() -> LoversDataState {
    LoversDataState::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_lovers_state() {
        let state = get_lovers_state();
        // 验证返回的实例可用
        assert_eq!(state.base.name, "lovers");
    }

    #[test]
    fn test_lovers_state_table_name() {
        let state = get_lovers_state();
        // 验证表名正确
        assert_eq!(state.datasync.table_name, "lovers");
    }
}
