//! Memcached 客户端
//!
//! 用于存储用户答题状态

use memcache::Client;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;

/// 答题状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizState {
    /// 科目ID
    pub idsubject: String,
    /// 年级
    pub grade: String,
    /// 科目名称
    pub subject: String,
    /// 题目ID
    pub question_id: String,
    /// 题目内容
    pub question: String,
    /// 标准答案
    pub standard_answer: String,
    /// 解析
    pub explanation: String,
    /// 题目难度分数
    pub score_difficulty: i32,
    /// 创建时间
    pub created_at: i64,
}

impl QuizState {
    /// 创建新的答题状态
    pub fn new(
        idsubject: String,
        grade: String,
        subject: String,
        question_id: String,
        question: String,
        standard_answer: String,
        explanation: String,
        score_difficulty: i32,
    ) -> Self {
        Self {
            idsubject,
            grade,
            subject,
            question_id,
            question,
            standard_answer,
            explanation,
            score_difficulty,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    /// 检查是否过期（默认5分钟）
    pub fn is_expired(&self, expire_secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.created_at > expire_secs
    }
}

/// Memcached 配置
#[derive(Debug, Clone)]
pub struct MemcachedConfig {
    pub host: String,
    pub port: u16,
    pub quiz_expire: u32,
}

impl Default for MemcachedConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11211,
            quiz_expire: 300,
        }
    }
}

/// Memcached 客户端
pub struct MemcachedClient {
    /// Memcached 客户端（可能不可用）
    client: Option<Client>,
    /// 内存缓存（降级时使用）
    memory_cache: Mutex<HashMap<String, (QuizState, i64)>>,
    /// 配置
    config: MemcachedConfig,
    /// 是否可用
    available: bool,
}

impl MemcachedClient {
    /// 创建新客户端
    pub fn new(config: MemcachedConfig) -> Result<Self, String> {
        // memcache 库使用 memcache:// 协议前缀
        let url = format!("memcache://{}:{}", config.host, config.port);
        match Client::connect(url.as_str()) {
            Ok(client) => {
                tracing::info!("Memcached 连接成功: {}:{}", config.host, config.port);
                Ok(Self {
                    client: Some(client),
                    memory_cache: Mutex::new(HashMap::new()),
                    config,
                    available: true,
                })
            }
            Err(e) => {
                Err(format!("连接 Memcached 失败: {}", e))
            }
        }
    }

    /// 创建不可用的客户端（降级模式）
    pub fn unavailable() -> Self {
        tracing::info!("使用内存缓存降级模式");
        Self {
            client: None,
            memory_cache: Mutex::new(HashMap::new()),
            config: MemcachedConfig::default(),
            available: false,
        }
    }

    /// 生成答题状态 key
    pub fn quiz_key(uid: &str) -> String {
        format!("daily_quiz:{}", uid)
    }

    /// 检查是否可用
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// 保存答题状态
    pub fn set_quiz_state(&self, uid: &str, state: &QuizState) -> Result<(), String> {
        let key = Self::quiz_key(uid);

        if self.available {
            if let Some(ref client) = self.client {
                let value = serde_json::to_string(state)
                    .map_err(|e| format!("序列化答题状态失败: {}", e))?;

                client
                    .set(&key, value, self.config.quiz_expire)
                    .map_err(|e| format!("写入 Memcached 失败: {}", e))?;

                tracing::info!("保存答题状态到Memcached: {} -> {}", key, state.question_id);
                return Ok(());
            }
        }

        // 降级到内存缓存
        let expire_time = chrono::Utc::now().timestamp() + self.config.quiz_expire as i64;
        let mut cache = self.memory_cache.lock().map_err(|e| format!("获取缓存锁失败: {}", e))?;
        cache.insert(key.clone(), (state.clone(), expire_time));
        tracing::info!("保存答题状态到内存缓存: {} -> {}", key, state.question_id);
        Ok(())
    }

    /// 获取答题状态
    pub fn get_quiz_state(&self, uid: &str) -> Result<Option<QuizState>, String> {
        let key = Self::quiz_key(uid);

        if self.available {
            if let Some(ref client) = self.client {
                let value: Option<String> = client
                    .get(&key)
                    .map_err(|e| format!("读取 Memcached 失败: {}", e))?;

                match value {
                    Some(json) => {
                        let state: QuizState = serde_json::from_str(&json)
                            .map_err(|e| format!("解析答题状态失败: {}", e))?;
                        tracing::info!("从Memcached获取答题状态: {} -> {}", key, state.question_id);
                        return Ok(Some(state));
                    }
                    None => {
                        tracing::info!("Memcached中答题状态不存在: {}", key);
                        return Ok(None);
                    }
                }
            }
        }

        // 降级到内存缓存
        let mut cache = self.memory_cache.lock().map_err(|e| format!("获取缓存锁失败: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        if let Some((state, expire_time)) = cache.get(&key) {
            if now < *expire_time {
                tracing::info!("从内存缓存获取答题状态: {} -> {}", key, state.question_id);
                return Ok(Some(state.clone()));
            } else {
                // 已过期，删除
                cache.remove(&key);
                tracing::info!("内存缓存中的答题状态已过期: {}", key);
            }
        }

        Ok(None)
    }

    /// 删除答题状态
    pub fn delete_quiz_state(&self, uid: &str) -> Result<(), String> {
        let key = Self::quiz_key(uid);

        if self.available {
            if let Some(ref client) = self.client {
                client
                    .delete(&key)
                    .map_err(|e| format!("删除 Memcached 失败: {}", e))?;
                tracing::info!("从Memcached删除答题状态: {}", key);
                return Ok(());
            }
        }

        // 降级到内存缓存
        let mut cache = self.memory_cache.lock().map_err(|e| format!("获取缓存锁失败: {}", e))?;
        cache.remove(&key);
        tracing::info!("从内存缓存删除答题状态: {}", key);
        Ok(())
    }

    /// 检查连接状态
    pub fn is_connected(&self) -> bool {
        if let Some(ref client) = self.client {
            client.stats().is_ok()
        } else {
            false
        }
    }
}

/// 全局 Memcached 客户端（延迟初始化）
pub static MEMCACHED_CLIENT: Lazy<Arc<MemcachedClient>> = Lazy::new(|| {
    // 从环境变量读取配置
    let config = MemcachedConfig {
        host: std::env::var("MEMCACHED_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("MEMCACHED_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(11211),
        quiz_expire: std::env::var("MEMCACHED_QUIZ_EXPIRE")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(300),
    };

    match MemcachedClient::new(config) {
        Ok(client) => {
            tracing::info!("Memcached 客户端初始化成功");
            Arc::new(client)
        }
        Err(e) => {
            tracing::warn!("Memcached 客户端初始化失败，使用内存缓存降级模式: {}", e);
            Arc::new(MemcachedClient::unavailable())
        }
    }
});

/// 获取 Memcached 客户端
pub fn get_memcached_client() -> &'static Arc<MemcachedClient> {
    &MEMCACHED_CLIENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quiz_state_new() {
        let state = QuizState::new(
            "subj001".to_string(),
            "三年级".to_string(),
            "数学".to_string(),
            "q001".to_string(),
            "1+1=?".to_string(),
            "2".to_string(),
            "简单加法".to_string(),
            50,
        );

        assert_eq!(state.grade, "三年级");
        assert_eq!(state.subject, "数学");
        assert!(!state.is_expired(300));
    }

    #[test]
    fn test_quiz_key() {
        let key = MemcachedClient::quiz_key("user123");
        assert_eq!(key, "daily_quiz:user123");
    }

    #[test]
    fn test_unavailable_client() {
        let client = MemcachedClient::unavailable();
        assert!(!client.is_available());

        let state = QuizState::new(
            "subj001".to_string(),
            "三年级".to_string(),
            "数学".to_string(),
            "q001".to_string(),
            "1+1=?".to_string(),
            "2".to_string(),
            "简单加法".to_string(),
            50,
        );

        // 测试内存缓存
        client.set_quiz_state("test_user", &state).unwrap();
        let retrieved = client.get_quiz_state("test_user").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().question_id, "q001");
    }
}
