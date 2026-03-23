//! 每日一炼核心模块
//!
//! 提供 LLM 调用、出题、判题功能

mod llm;

pub use llm::{generate_question, judge_answer, QuizGenerateResult, QuizJudgeResult};
