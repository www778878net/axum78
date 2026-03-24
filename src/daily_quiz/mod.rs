//! 每日一炼核心模块
//!
//! 提供 LLM 调用、出题、判题功能，以及积分更新和答题记录

use base::ProjectPath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 导出数据库结构体
pub mod db {
    pub use crate::database::daily::{subject::DailySubject, user_score::DailyUserScore, answer_record::DailyAnswerRecord};
}

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

/// ModelScope配置
#[derive(Debug, Clone)]
struct ModelScopeConfig {
    pub model: String,
    pub api_url: String,
    pub api_key: String,
}

impl ModelScopeConfig {
    fn from_ini() -> Result<Self, String> {
        let project_path = ProjectPath::find()
            .map_err(|e| format!("查找项目根目录失败: {}", e))?;
        let ini_config = project_path.load_ini_config()
            .map_err(|e| format!("加载配置文件失败: {}", e))?;

        let modelscope = ini_config.get("modelscope")
            .ok_or_else(|| "配置文件中缺少[modelscope] section".to_string())?;

        Ok(ModelScopeConfig {
            model: modelscope.get("model")
                .map(|s| s.to_string())
                .ok_or_else(|| "缺少model配置".to_string())?,
            api_url: modelscope.get("api_url")
                .map(|s| s.to_string())
                .ok_or_else(|| "缺少api_url配置".to_string())?,
            api_key: modelscope.get("api_key")
                .map(|s| s.to_string())
                .ok_or_else(|| "缺少api_key配置".to_string())?,
        })
    }
}

/// 调用ModelScope API
async fn call_modelscope(prompt: &str) -> Result<serde_json::Value, String> {
    let config = ModelScopeConfig::from_ini()?;

    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "stream": false
    });

    let response = client
        .post(&config.api_url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("发送请求失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API返回错误 {}: {}", status, text));
    }

    let json: serde_json::Value = response.json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(json)
}

/// 读取提示词模板
fn load_prompt(template_name: &str, params: HashMap<&str, &str>) -> Result<String, String> {
    let project_path = ProjectPath::find()
        .map_err(|e| format!("查找项目根目录失败: {}", e))?;
    let prompt_path = project_path.join("refs").join(template_name);

    let content = std::fs::read_to_string(&prompt_path)
        .map_err(|e| format!("读取提示词文件失败: {}", e))?;

    let mut result = content;
    for (key, value) in params {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }

    Ok(result)
}

/// 生成题目（真正调用LLM）
pub async fn generate_question(
    grade: &str,
    subject: &str,
    current_score: Option<i32>,
) -> Result<QuizGenerateResult, String> {
    tracing::info!("LLM出题: grade={}, subject={}, current_score={:?}", grade, subject, current_score);

    // 计算目标难度范围
    let score_min = current_score.unwrap_or(50).saturating_sub(5);
    let score_max = current_score.unwrap_or(50).saturating_add(5);
    let target_min = ((score_min + score_max) / 2).saturating_sub(5);
    let target_max = ((score_min + score_max) / 2).saturating_add(5);

    // 加载出题提示词模板
    let mut params = HashMap::new();
    params.insert("grade", grade);
    params.insert("subject", subject);
    params.insert("score_min", &score_min.to_string());
    params.insert("score_max", &score_max.to_string());
    params.insert("target_min", &target_min.to_string());
    params.insert("target_max", &target_max.to_string());

    let prompt = load_prompt("daily_quiz_prompt.md", params)?;

    // 调用LLM
    let response = call_modelscope(&prompt).await?;

    // 解析返回的JSON
    let choices = response.get("choices")
        .and_then(|v| v.get(0))
        .ok_or_else(|| "响应缺少choices字段".to_string())?;

    let message = choices.get("message")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "响应缺少message.content字段".to_string())?;

    // 提取JSON部分（可能包含markdown代码块）
    let json_str = extract_json_from_text(message);

    let result: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析LLM返回JSON失败: {}, content: {}", e, json_str))?;

    Ok(QuizGenerateResult {
        question_id: format!("q_{}", chrono::Utc::now().timestamp()),
        question: result.get("question")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "缺少question字段".to_string())?,
        hint: result.get("hint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        standard_answer: result.get("answer")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "缺少answer字段".to_string())?,
        explanation: result.get("explanation")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "缺少explanation字段".to_string())?,
        score_difficulty: result.get("score_difficulty")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32)
            .ok_or_else(|| "缺少或无效的score_difficulty字段".to_string())?,
    })
}

/// 判题（真正调用LLM）
pub async fn judge_answer(
    uid: &str,
    grade: &str,
    subject: &str,
    question: &str,
    standard_answer: &str,
    explanation: &str,
    user_answer: &str,
    current_score: i32,
    question_difficulty: i32,
) -> Result<QuizJudgeResult, String> {
    tracing::info!("LLM判题: uid={}, grade={}, subject={}, answer={}", uid, grade, subject, user_answer);

    // 加载判题提示词模板
    let mut params = HashMap::new();
    params.insert("grade", grade);
    params.insert("subject", subject);
    params.insert("question", question);
    params.insert("standard_answer", standard_answer);
    params.insert("user_answer", user_answer);
    params.insert("current_score", &current_score.to_string());
    params.insert("question_difficulty", &question_difficulty.to_string());
    params.insert("explanation", explanation);

    let prompt = load_prompt("daily_judge_prompt.md", params)?;

    // 调用LLM
    let response = call_modelscope(&prompt).await?;

    // 解析返回的JSON
    let choices = response.get("choices")
        .and_then(|v| v.get(0))
        .ok_or_else(|| "响应缺少choices字段".to_string())?;

    let message = choices.get("message")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "响应缺少message.content字段".to_string())?;

    // 提取JSON部分
    let json_str = extract_json_from_text(message);

    let result: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析LLM返回JSON失败: {}, content: {}", e, json_str))?;

    let is_correct = result.get("is_correct")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "缺少is_correct字段".to_string())?;

    let score_change = result.get("score_change")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .ok_or_else(|| "缺少score_change字段".to_string())?;

    let feedback = result.get("feedback")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "缺少feedback字段".to_string())?;

    let explanation = result.get("explanation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "缺少explanation字段".to_string())?;

    let new_score = result.get("new_score")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .ok_or_else(|| "缺少new_score字段".to_string())?;

    let points_change = result.get("points_change")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .ok_or_else(|| "缺少points_change字段".to_string())?;

    Ok(QuizJudgeResult {
        is_correct: *is_correct,
        score_change: *score_change,
        feedback: feedback.clone(),
        explanation: explanation.clone(),
        new_score: *new_score,
        points_change: *points_change,
    })
}

/// 从文本中提取JSON（处理markdown代码块）
fn extract_json_from_text(text: &str) -> String {
    // 查找```json ... ```块
    if let Some(start) = text.find("```json") {
        if let Some(content_start) = text[start..].find('\n') {
            let start_idx = start + content_start + 1;
            if let Some(end) = text[start_idx..].find("```") {
                return text[start_idx..start_idx + end].trim().to_string();
            }
        }
    }

    // 查找```...```块
    if let Some(start) = text.find("```") {
        if let Some(content_start) = text[start..].find('\n') {
            let start_idx = start + content_start + 1;
            if let Some(end) = text[start_idx..].find("```") {
                return text[start_idx..start_idx + end].trim().to_string();
            }
        }
    }

    // 如果没有代码块，尝试找到第一个{和最后一个}之间的内容
    if let Some(first_brace) = text.find('{') {
        if let Some(last_brace) = text.rfind('}') {
            if first_brace < last_brace {
                return text[first_brace..=last_brace].to_string();
            }
        }
    }

    // 返回原始文本
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_text() {
        let text = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_text(text), "{\"key\": \"value\"}");

        let text2 = "Some text {\"key\": \"value\"} more text";
        assert_eq!(extract_json_from_text(text2), "{\"key\": \"value\"}");
    }
}
