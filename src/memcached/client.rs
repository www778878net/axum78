//! Memcached 客户端
//!
//! 用于存储用户答题状态

use memcache::{Client, MemcacheError};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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
    client: Client,
    config: MemcachedConfig,
}

impl MemcachedClient {
    /// 创建新客户端
    pub fn new(config: MemcachedConfig) -> Result<Self, String> {
        let url = format!("tcp://{}:{}", config.host, config.port);
        let client = Client::connect(url.as_str())
            .map_err(|e| format!("连接 Memcached 失败: {}", e))?;

        Ok(Self { client, config })
    }

    /// 生成答题状态 key
    pub fn quiz_key(uid: &str) -> String {
        format!("daily_quiz:{}", uid)
    }

    /// 保存答题状态
    pub fn set_quiz_state(&self, uid: &str, state: &QuizState) -> Result<(), String> {
        let key = Self::quiz_key(uid);
        let value = serde_json::to_string(state)
            .map_err(|e| format!("序列化答题状态失败: {}", e))?;

        self.client
            .set(&key, value, self.config.quiz_expire)
            .map_err(|e| format!("写入 Memcached 失败: {}", e))?;

        tracing::info!("保存答题状态: {} -> {}", key, state.question_id);
        Ok(())
    }

    /// 获取答题状态
    pub fn get_quiz_state(&self, uid: &str) -> Result<Option<QuizState>, String> {
        let key = Self::quiz_key(uid);

        let value: Option<String> = self.client
            .get(&key)
            .map_err(|e| format!("读取 Memcached 失败: {}", e))?;

        match value {
            Some(json) => {
                let state: QuizState = serde_json::from_str(&json)
                    .map_err(|e| format!("解析答题状态失败: {}", e))?;
                tracing::info!("获取答题状态: {} -> {}", key, state.question_id);
                Ok(Some(state))
            }
            None => {
                tracing::info!("答题状态不存在: {}", key);
                Ok(None)
            }
        }
    }

    /// 删除答题状态
    pub fn delete_quiz_state(&self, uid: &str) -> Result<(), String> {
        let key = Self::quiz_key(uid);

        self.client
            .delete(&key)
            .map_err(|e| format!("删除 Memcached 失败: {}", e))?;

        tracing::info!("删除答题状态: {}", key);
        Ok(())
    }

    /// 检查连接状态
    pub fn is_connected(&self) -> bool {
        self.client.stats().is_ok()
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
            tracing::error!("Memcached 客户端初始化失败: {}", e);
            // 返回一个占位客户端（实际不可用）
            panic!("Memcached 客户端初始化失败: {}", e);
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
}
