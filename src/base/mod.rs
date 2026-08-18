//! Base - 基础库（自包含副本，不依赖 rustbase crate）
//!
//! 提供日志、请求上下文、项目路径等通用组件。
//! axum78 准备弃用，因此将其所依赖的 rustbase 模块复制进来，避免
//! 后续 rustbase 重构破坏 axum78。

pub mod mylogger;
pub mod project_path;
pub mod upinfo;

// MyLogger
pub use mylogger::{MyLogger, LogLevel, Environment, get_logger};

// ProjectPath
pub use project_path::{ProjectPath, load_ini_from_path, parse_ini_content};

// UpInfo
pub use upinfo::{UpInfo, UpInfoError, Response};
