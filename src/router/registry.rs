//! 全局 Controller 注册表
//!
//! 模仿 koa78base 的 ControllerLoader，所有 handler 按 `apisys/apimicro/apiobj` 路径注册，
//! api_handler 查表转发，无需手动 match 分支。
//!
//! 用法：
//!   registry::register("apisvc/backsvc/datasync", Arc::new(DatasyncController));
//!   let ctrl = registry::lookup("apisvc/backsvc/datasync");

use crate::router::Controller78;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

type ControllerMap = HashMap<String, Arc<dyn Controller78>>;

static REGISTRY: Lazy<RwLock<ControllerMap>> = Lazy::new(|| RwLock::new(HashMap::new()));

/// 注册 controller
pub fn register(path: &str, controller: Arc<dyn Controller78>) {
    REGISTRY.write().unwrap().insert(path.to_string(), controller);
}

/// 查找 controller（大小写不敏感：先精确匹配，再小写匹配）
pub fn lookup(path: &str) -> Option<Arc<dyn Controller78>> {
    let reg = REGISTRY.read().unwrap();
    reg.get(path)
        .cloned()
        .or_else(|| reg.get(&path.to_lowercase()).cloned())
}
