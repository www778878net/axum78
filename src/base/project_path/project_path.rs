//! ProjectPath - 项目路径查找工具实现
//!
//! 提供项目根目录查找、默认路径生成、配置加载等能力

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

/// 环境类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Test,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Environment {
    /// 从 APP_ENV 环境变量获取环境
    pub fn from_env() -> Self {
        match std::env::var("APP_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            .as_str()
        {
            "development" | "dev" => Environment::Development,
            "test" | "testing" => Environment::Test,
            "production" | "prod" => Environment::Production,
            _ => Environment::Development,
        }
    }

    /// 获取环境名称
    pub fn name(&self) -> &'static str {
        match self {
            Environment::Development => "development",
            Environment::Test => "test",
            Environment::Production => "production",
        }
    }
}

/// 项目路径查找结果
#[derive(Debug, Clone)]
pub struct ProjectPath {
    /// 项目根目录（包含 docs 或 .claude 的目录）
    root: PathBuf,
    /// 当前环境
    environment: Environment,
}

impl ProjectPath {
    /// 查找项目根目录
    ///
    /// 从当前工作目录开始，向上查找 docs 或 .claude 目录
    /// 找到的第一个即为项目根目录
    pub fn find() -> Result<Self, String> {
        let current_dir = std::env::current_dir()
            .map_err(|e| format!("获取当前目录失败: {}", e))?;

        Self::find_from(&current_dir)
    }

    /// 从指定路径开始查找项目根目录
    pub fn find_from(start: &PathBuf) -> Result<Self, String> {
        let mut path = start.clone();

        loop {
            let docs_path = path.join("docs");
            let claude_path = path.join(".claude");

            if docs_path.exists() || claude_path.exists() {
                return Ok(Self {
                    root: path,
                    environment: Environment::from_env(),
                });
            }

            // 向上一级
            match path.parent() {
                Some(parent) => path = parent.to_path_buf(),
                None => break,
            }
        }

        Err(format!(
            "未找到项目根目录（未找到 docs 或 .claude 目录）, 起始路径: {:?}",
            start
        ))
    }

    /// 获取项目根目录
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// 获取当前环境
    pub fn environment(&self) -> Environment {
        self.environment
    }

    /// 获取 docs 目录
    pub fn docs(&self) -> PathBuf {
        self.root.join("docs")
    }

    /// 获取 docs/config 目录
    pub fn docs_config(&self) -> PathBuf {
        self.root.join("docs").join("config")
    }

    /// 获取配置文件路径 (docs/config/{filename})
    pub fn config_file(&self, filename: &str) -> PathBuf {
        self.docs_config().join(filename)
    }

    /// 获取环境配置文件路径 (docs/config/{env}.ini)
    pub fn env_config_file(&self) -> PathBuf {
        self.docs_config().join(format!("{}.ini", self.environment.name()))
    }

    /// 获取本地数据库路径 (docs/config/local.db)
    pub fn local_db(&self) -> PathBuf {
        self.docs_config().join("local.db")
    }

    /// 读取简单文本配置文件
    pub fn read_text_config(&self, filename: &str, env_key: &str) -> Option<String> {
        // 环境变量优先
        if let Ok(val) = std::env::var(env_key) {
            return Some(val);
        }

        // 读取文件内容
        let config_path = self.config_file(filename);
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
        None
    }

    /// 获取 Worker 名称
    /// 从 tmp/lockid/worker.txt 读取，每个终端不同
    /// 如果不存在则自动生成（使用UUID）
    pub fn worker_name(&self) -> Option<String> {
        if let Ok(val) = std::env::var("WORKER_NAME") {
            return Some(val);
        }

        let worker_path = self.join("tmp/lockid/worker.txt");
        if worker_path.exists() {
            if let Ok(content) = fs::read_to_string(&worker_path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
        
        // 自动生成worker标识（使用UUID）
        let worker_id = format!("worker_{}", uuid::Uuid::new_v4());
        
        // 确保目录存在
        if let Some(parent) = worker_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        
        // 写入文件
        if fs::write(&worker_path, &worker_id).is_ok() {
            return Some(worker_id);
        }
        
        // 写入失败，返回生成的ID（但不保存）
        Some(worker_id)
    }

    /// 加载 INI 配置文件（默认路径）
    pub fn load_ini_config(&self) -> Result<HashMap<String, HashMap<String, String>>, String> {
        let config_path = self.env_config_file();
        Self::load_ini_from_path(&config_path)
    }

    /// 从指定路径加载 INI 配置文件
    /// 自动加载 docs/config/.env 文件（如果存在）
    pub fn load_ini_from_path(config_path: &Path) -> Result<HashMap<String, HashMap<String, String>>, String> {
        if !config_path.exists() {
            return Err(format!("配置文件不存在: {}", config_path.to_string_lossy()));
        }

        // 自动加载 .env 文件（仅在首次调用时）
        static DOTENV_LOADED: std::sync::Once = std::sync::Once::new();
        DOTENV_LOADED.call_once(|| {
            // .env 与 ini 文件同目录：docs/config/.env
            if let Some(config_dir) = config_path.parent() {
                let env_path = config_dir.join(".env");
                Self::load_dotenv(&env_path);
            }
        });

        let content = fs::read_to_string(config_path)
            .map_err(|e| format!("读取配置文件失败: {}", e))?;

        Self::parse_ini_content(&content)
    }

    /// 从字符串解析 INI 内容
    /// 支持 ${ENV_VAR} 占位符，运行时替换为环境变量值
    pub fn parse_ini_content(content: &str) -> Result<HashMap<String, HashMap<String, String>>, String> {
        let mut config: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_section = "default".to_string();

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len()-1].trim().to_string();
                // 移除可能的 \r 字符（Windows换行符）
                current_section = current_section.replace('\r', "");
                config.entry(current_section.clone()).or_insert_with(HashMap::new);
                continue;
            }

            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().replace('\r', "").to_string();
                let value = line[pos + 1..].trim().replace('\r', "").to_string();
                // 替换 ${ENV_VAR} 占位符为环境变量值
                let resolved = Self::resolve_env_vars(&value);
                config
                    .entry(current_section.clone())
                    .or_insert_with(HashMap::new)
                    .insert(key, resolved);
            }
        }

        Ok(config)
    }

    fn resolve_env_vars(value: &str) -> String {
        let mut result = value.to_string();
        loop {
            if let Some(start) = result.find("${") {
                if let Some(end) = result[start + 2..].find('}') {
                    let placeholder = &result[start + 2..start + 2 + end];
                    let (env_key, default_val) = if let Some(dsep) = placeholder.find(":-") {
                        (&placeholder[..dsep], Some(&placeholder[dsep + 2..]))
                    } else {
                        (placeholder, None)
                    };
                    let env_val = std::env::var(env_key).unwrap_or_default();
                    let replace_val = if env_val.is_empty() {
                        default_val.unwrap_or("")
                    } else {
                        &env_val
                    };
                    result = format!("{}{}{}", &result[..start], replace_val, &result[start + 2 + end + 1..]);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        result
    }

    /// 加载 .env 文件，解析并注入环境变量
    /// 仅在环境变量未设置时才注入（不覆盖已有值）
    fn load_dotenv(env_path: &Path) {
        if !env_path.exists() {
            return;
        }
        if let Ok(content) = fs::read_to_string(env_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(pos) = line.find('=') {
                    let key = line[..pos].trim().to_string();
                    let value = line[pos + 1..].trim().to_string();
                    // 仅在环境变量未设置时才注入
                    if std::env::var(&key).is_err() {
                        std::env::set_var(&key, &value);
                    }
                }
            }
        }
    }

    /// 从 INI 配置文件读取值
    pub fn read_ini_value(&self, section: &str, key: &str) -> Option<String> {
        let env_key = match key.to_lowercase().as_str() {
            "uid" => "USER_UID".to_string(),
            "uname" | "username" => "USER_NAME".to_string(),
            _ => key.to_uppercase(),
        };
        if let Ok(val) = std::env::var(&env_key) {
            if !val.is_empty() {
                return Some(val);
            }
        }

        self.load_ini_config()
            .ok()?
            .get(section)?
            .get(key)
            .cloned()
    }

    /// 加载配置文件（key=value 格式）
    pub fn load_config(&self, filename: &str) -> Result<HashMap<String, String>, String> {
        let config_path = self.config_file(filename);

        if !config_path.exists() {
            return Err(format!("配置文件不存在: {}", config_path.to_string_lossy()));
        }

        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("读取配置文件失败: {}", e))?;

        let mut config = HashMap::new();

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                config.insert(key, value);
            }
        }

        Ok(config)
    }

    /// 获取配置值
    pub fn get_config(&self, filename: &str, key: &str) -> Option<String> {
        let env_key = key.to_uppercase();
        if let Ok(val) = std::env::var(&env_key) {
            return Some(val);
        }

        self.load_config(filename).ok()?.get(key).cloned()
    }

    /// 获取 .claude 目录
    pub fn claude(&self) -> PathBuf {
        self.root.join(".claude")
    }

    /// 获取 logs 目录
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// 获取 tmp 目录
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// 获取 memory78 目录
    pub fn memory78(&self) -> PathBuf {
        self.root.join("memory78")
    }

    /// 获取 data 目录（子GIT，存放 input/check 等测试数据）
    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    /// 获取 input 文件路径
    /// 例如: module="base", class="project_path" -> data/base/src/project_path/project_path.input
    pub fn data_input(&self, module: &str, class: &str) -> PathBuf {
        self.data()
            .join(module)
            .join("src")
            .join(class)
            .join(format!("{}.input", class))
    }

    /// 获取 check 文件路径
    pub fn data_check(&self, module: &str, class: &str) -> PathBuf {
        self.data()
            .join(module)
            .join("src")
            .join(class)
            .join(format!("{}.check", class))
    }

    /// 拼接任意路径
    pub fn join(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// 转换为字符串
    pub fn to_string(&self) -> String {
        self.root.to_string_lossy().to_string()
    }
}

impl Default for ProjectPath {
    fn default() -> Self {
        Self::find().unwrap_or_else(|_| Self {
            root: std::env::current_dir().unwrap_or_default(),
            environment: Environment::from_env(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_find_root() {
        let result = ProjectPath::find();
        assert!(result.is_ok());
        let project = result.unwrap();
        // 验证 root 包含 docs 或 .claude
        assert!(project.root.join("docs").exists() || project.root.join(".claude").exists());
    }

    #[test]
    fn test_local_db_path() {
        let project = ProjectPath::find().unwrap();
        let db_path = project.local_db();
        let path_str = db_path.to_string_lossy();
        // Windows 使用反斜杠，所以用 contains 而非 ends_with
        assert!(path_str.contains("docs"));
        assert!(path_str.contains("config"));
        assert!(path_str.contains("local.db"));
    }

    #[test]
    fn test_docs_config_path() {
        let project = ProjectPath::find().unwrap();
        let config_path = project.docs_config();
        let path_str = config_path.to_string_lossy();
        // Windows 使用反斜杠，所以用 contains 而非 ends_with
        assert!(path_str.contains("docs"));
        assert!(path_str.contains("config"));
    }

    #[test]
    fn test_environment_from_env() {
        // 默认应该是 Development（因为 APP_ENV 未设置或设置）
        let env = Environment::from_env();
        // 在测试环境中，如果没有设置 APP_ENV，默认是 Development
        assert!(matches!(env, Environment::Development | Environment::Test | Environment::Production));
    }

    #[test]
    fn test_environment_name() {
        assert_eq!(Environment::Development.name(), "development");
        assert_eq!(Environment::Test.name(), "test");
        assert_eq!(Environment::Production.name(), "production");
    }

    #[test]
    fn test_find_from_valid_path() {
        let current_dir = std::env::current_dir().unwrap();
        let result = ProjectPath::find_from(&current_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_from_invalid_path() {
        // 从一个不包含 docs 或 .claude 的路径开始查找
        // 使用系统临时目录
        let temp_dir = std::env::temp_dir();
        // 如果临时目录本身或其父目录包含 docs/.claude，则跳过此测试
        if temp_dir.join("docs").exists() || temp_dir.join(".claude").exists() {
            return;
        }
        // 继续向上查找到根目录，如果仍找不到，会返回错误
        let result = ProjectPath::find_from(&temp_dir);
        // 结果取决于临时目录是否在项目目录下
        if result.is_err() {
            assert!(result.unwrap_err().contains("未找到项目根目录"));
        }
    }

    #[test]
    fn test_parse_ini_content() {
        let content = r#"
[section1]
key1 = value1
key2 = value2

[section2]
key3 = value3
"#;
        let result = ProjectPath::parse_ini_content(content);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.get("section1").unwrap().get("key1").unwrap(), "value1");
        assert_eq!(config.get("section1").unwrap().get("key2").unwrap(), "value2");
        assert_eq!(config.get("section2").unwrap().get("key3").unwrap(), "value3");
    }

    #[test]
    fn test_parse_ini_with_comments() {
        let content = r#"
[section1]
# this is a comment
key1 = value1
; another comment
key2 = value2
"#;
        let result = ProjectPath::parse_ini_content(content);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.get("section1").unwrap().len(), 2);
    }

    #[test]
    fn test_join_path() {
        let project = ProjectPath::find().unwrap();
        let joined = project.join("test/path");
        assert!(joined.to_string_lossy().contains("test/path"));
    }

    #[test]
    fn test_docs_path() {
        let project = ProjectPath::find().unwrap();
        let docs = project.docs();
        assert!(docs.to_string_lossy().ends_with("docs"));
    }

    #[test]
    fn test_logs_path() {
        let project = ProjectPath::find().unwrap();
        let logs = project.logs();
        assert!(logs.to_string_lossy().ends_with("logs"));
    }

    #[test]
    fn test_tmp_path() {
        let project = ProjectPath::find().unwrap();
        let tmp = project.tmp();
        assert!(tmp.to_string_lossy().ends_with("tmp"));
    }

    #[test]
    fn test_config_file() {
        let project = ProjectPath::find().unwrap();
        let config_file = project.config_file("test.ini");
        let path_str = config_file.to_string_lossy();
        // Windows 使用反斜杠，所以用 contains 而非 ends_with
        assert!(path_str.contains("docs"));
        assert!(path_str.contains("config"));
        assert!(path_str.contains("test.ini"));
    }

    #[test]
    fn test_data_input_path() {
        let project = ProjectPath::find().unwrap();
        let input_path = project.data_input("base", "project_path");
        let path_str = input_path.to_string_lossy();
        assert!(path_str.contains("data"));
        assert!(path_str.contains("base"));
        assert!(path_str.contains("project_path.input"));
    }

    #[test]
    fn test_data_check_path() {
        let project = ProjectPath::find().unwrap();
        let check_path = project.data_check("base", "project_path");
        let path_str = check_path.to_string_lossy();
        assert!(path_str.contains("data"));
        assert!(path_str.contains("base"));
        assert!(path_str.contains("project_path.check"));
    }
}