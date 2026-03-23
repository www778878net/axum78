//! 每日一炼核心模块
//!
//! 提供 LLM 调用、出题、判题功能

use serde::{Deserialize, Serialize};

/// 出题响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizGenerateResult {
    pub question_id: String,
    pub question: String,
    pub hint: Option<String>,
    pub standard_answer: String,
    pub explanation: String,
    pub score_difficulty: i32,
}

/// 判题响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizJudgeResult {
    pub is_correct: bool,
    pub score_change: i32,
    pub feedback: String,
    pub explanation: String,
    pub new_score: i32,
    pub points_change: i32,
}

/// 出题（简化版，直接返回模拟题目）
pub async fn generate_question(
    grade: &str,
    subject: &str,
    _current_score: Option<i32>,
) -> Result<QuizGenerateResult, String> {
    // 模拟出题
    tracing::info!("模拟出题: grade={}, subject={}", grade, subject);
    
    Ok(QuizGenerateResult {
        question_id: format!("q_{}", chrono::Utc::now().timestamp()),
        question: format!("{}{}练习题：小明有15颗糖果，分给5个朋友，每人几颗？", grade, subject),
        hint: Some("用除法计算".to_string()),
        standard_answer: "3".to_string(),
        explanation: "15 ÷ 5 = 3，每人分到3颗糖果".to_string(),
        score_difficulty: 60,
    })
}

/// 判题（简化版）
pub async fn judge_answer(
    _uid: &str,
    grade: &str,
    subject: &str,
    question: &str,
    standard_answer: &str,
    _explanation: &str,
    user_answer: &str,
    current_score: i32,
    _question_difficulty: i32,
) -> Result<QuizJudgeResult, String> {
    tracing::info!("模拟判题: grade={}, subject={}, answer={}", grade, subject, user_answer);
    
    let is_correct = user_answer.trim() == standard_answer.trim();
    let score_change = if is_correct { 2 } else { -1 };
    let points_change = if is_correct { 20 } else { 5 };
    
    Ok(QuizJudgeResult {
        is_correct,
        score_change,
        feedback: if is_correct { "回答正确！".to_string() } else { format!("回答错误，正确答案是: {}", standard_answer) },
        explanation: format!("题目：{}\n解析：这是一道基础题", question),
        new_score: (current_score + score_change).clamp(0, 100),
        points_change,
    })
}