//! Memcached 客户端模块
//!
//! 用于存储用户答题状态

mod client;

pub use client::{MemcachedClient, QuizState, MEMCACHED_CLIENT, get_memcached_client};
