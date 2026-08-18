//! ProjectPath - 项目路径查找工具
//!
//! 向上查找 docs 或 .claude 目录，确定项目根目录

pub mod project_path;

pub use project_path::{ProjectPath, Environment};

/// 从指定路径加载 INI 配置文件
pub fn load_ini_from_path<P: AsRef<std::path::Path>>(path: P) -> Result<std::collections::HashMap<String, std::collections::HashMap<String, String>>, String> {
    ProjectPath::load_ini_from_path(path.as_ref())
}

/// 从字符串解析 INI 内容
pub fn parse_ini_content(content: &str) -> Result<std::collections::HashMap<String, std::collections::HashMap<String, String>>, String> {
    ProjectPath::parse_ini_content(content)
}
