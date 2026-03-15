//! 数据同步模块
//!
//! 参考 logsvc 的同步机制实现

mod data_sync;
mod sync_api;

#[cfg(test)]
mod sync_client_test;

#[cfg(test)]
mod e2e_sync_test;

#[cfg(test)]
mod doc_test_plan;

pub use data_sync::{DataSync, SyncConfig, SyncResult};
pub use sync_api::{create_router, AppState};
