//! MyLogger - 日志组件实现
//!
//! 工作流日志类：
//! - 全局日志：logs/project/project.log 和 logs/project/detail.log（固定，不变）
//! - 工作流日志：logs/{wfname}/{wfname}.log 或 logs/{myname}/{myname}.log

use std::ffi::{CStr, CString};
use std::io::{Seek, SeekFrom};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs::{self, OpenOptions, File};
use std::io::Write;
use std::collections::HashMap;

use chrono::{DateTime, Local, Duration};
use parking_lot::RwLock;

// =============================================================================
// 日志级别
// =============================================================================

/// 日志级别枚举
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Detail = 5,
    Debug = 10,
    Info = 20,
    Warn = 30,
    Error = 40,
}

impl LogLevel {
    pub fn from_i32(value: i32) -> Self {
        match value {
            5 => LogLevel::Detail,
            10 => LogLevel::Debug,
            20 => LogLevel::Info,
            30 => LogLevel::Warn,
            40 => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Detail => "DETAIL",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

// =============================================================================
// 环境类型
// =============================================================================

/// 运行环境
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Production,
    Development,
    Test,
}

impl std::str::FromStr for Environment {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "development" => Ok(Environment::Development),
            "test" => Ok(Environment::Test),
            _ => Ok(Environment::Production),
        }
    }
}

impl Environment {
    pub fn from_str_legacy(s: &str) -> Self {
        s.parse().unwrap_or(Environment::Production)
    }

    pub fn console_level(&self) -> LogLevel {
        match self {
            Environment::Production => LogLevel::Info,
            Environment::Development => LogLevel::Detail,
            Environment::Test => LogLevel::Debug,
        }
    }

    pub fn file_level(&self) -> LogLevel {
        match self {
            Environment::Production => LogLevel::Info,
            Environment::Development => LogLevel::Detail,
            Environment::Test => LogLevel::Debug,
        }
    }
}

// =============================================================================
// 全局日志管理器（单例）
// =============================================================================

struct GlobalLogger {
    project_root: PathBuf,
    project_file: Arc<Mutex<Option<File>>>,
    detail_file: Arc<Mutex<Option<File>>>,
    write_lock: Arc<Mutex<()>>,
}

impl GlobalLogger {
    fn new(project_root: &Path) -> Self {
        let logs_dir = project_root.join("logs").join("project");
        fs::create_dir_all(&logs_dir).ok();

        let project_path = logs_dir.join("project.log");
        let detail_path = logs_dir.join("detail.log");

        // 归档现有 project.log
        if project_path.exists() && project_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            let archive_path = logs_dir.join(format!("project_{}.log", timestamp));
            fs::rename(&project_path, &archive_path).ok();
        }

        // 备份 detail.log -> detail2.log
        if detail_path.exists() {
            let backup_path = logs_dir.join("detail2.log");
            fs::copy(&detail_path, &backup_path).ok();
        }

        // 尝试打开项目日志文件
        let project_file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&project_path)
        {
            Ok(file) => Some(file),
            Err(e) => {
                eprintln!("警告: 无法打开 project.log: {}. 将只使用控制台日志.", e);
                None
            }
        };

        // 尝试打开详细日志文件
        let detail_file = match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&detail_path)
        {
            Ok(file) => Some(file),
            Err(e) => {
                eprintln!("警告: 无法打开 detail.log: {}. 将只使用控制台日志.", e);
                None
            }
        };

        Self {
            project_root: project_root.to_path_buf(),
            project_file: Arc::new(Mutex::new(project_file)),
            detail_file: Arc::new(Mutex::new(detail_file)),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    fn write_project(&self, message: &str, level: LogLevel) {
        if level < LogLevel::Info {
            return;
        }
        let _guard = self.write_lock.lock().ok();
        if let Ok(mut file_opt) = self.project_file.lock() {
            if let Some(ref mut file) = *file_opt {
                let _ = file.write_all(message.as_bytes());
            }
        }
    }

    fn write_detail(&self, message: &str) {
        let _guard = self.write_lock.lock().ok();
        if let Ok(mut file_opt) = self.detail_file.lock() {
            if let Some(ref mut file) = *file_opt {
                // NOTE: 单进程多线程安全（有 Mutex 保护），但多进程会有冲突
                let _ = file.seek(SeekFrom::End(0));
                let _ = file.write_all(message.as_bytes());
            }
        }
    }

    fn cleanup_old_logs(&self, retention_days: i32) {
        if retention_days <= 0 {
            return;
        }

        let logs_dir = self.project_root.join("logs").join("project");
        let cutoff = Local::now() - Duration::days(retention_days as i64);

        if let Ok(entries) = fs::read_dir(&logs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                if path.is_file() && filename.starts_with("project_") && filename.ends_with(".log") {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let modified_dt: DateTime<Local> = modified.into();
                            if modified_dt < cutoff && fs::remove_file(&path).is_ok() {
                                println!("删除过期归档日志: project/{}", filename);
                            }
                        }
                    }
                }
            }
        }
    }

    fn get_project_root(&self) -> &Path {
        &self.project_root
    }

    /// 启动周期性日志清理（后台线程，每小时一次）
    fn start_periodic_cleanup(project_root: PathBuf, retention_days: i32) {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if STARTED.swap(true, Ordering::SeqCst) {
            return; // 已启动
        }
        if retention_days <= 0 {
            return;
        }

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600)); // 每小时
                let cutoff = Local::now() - Duration::days(retention_days as i64);

                // 清理 project/ 归档日志
                let project_dir = project_root.join("logs").join("project");
                if let Ok(entries) = fs::read_dir(&project_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() { continue; }
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        if !(filename.starts_with("project_") && filename.ends_with(".log")) {
                            continue;
                        }
                        if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                            let modified_dt: DateTime<Local> = modified.into();
                            if modified_dt < cutoff {
                                fs::remove_file(&path).ok();
                            }
                        }
                    }
                }

                // 清理所有 workflow 子目录
                let logs_root = project_root.join("logs");
                if let Ok(entries) = fs::read_dir(&logs_root) {
                    for entry in entries.flatten() {
                        let dir = entry.path();
                        if !dir.is_dir() { continue; }
                        let _dir_name = dir.file_name().unwrap_or_default().to_string_lossy();
                        if _dir_name == "project" { continue; }
                        if let Ok(files) = fs::read_dir(&dir) {
                            for file_entry in files.flatten() {
                                let fp = file_entry.path();
                                if !fp.is_file() { continue; }
                                if let Ok(modified) = file_entry.metadata().and_then(|m| m.modified()) {
                                    let modified_dt: DateTime<Local> = modified.into();
                                    if modified_dt < cutoff {
                                        fs::remove_file(&fp).ok();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

static GLOBAL_LOGGER: OnceLock<Arc<GlobalLogger>> = OnceLock::new();

fn get_global_logger() -> Arc<GlobalLogger> {
    GLOBAL_LOGGER.get_or_init(|| {
        let project_root = find_project_root();
        Arc::new(GlobalLogger::new(&project_root))
    }).clone()
}

fn find_project_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("docs").exists() || cwd.join(".claude").exists() {
        return cwd;
    }
    let mut current = cwd.clone();
    while let Some(parent) = current.parent() {
        if parent.join("docs").exists() || parent.join(".claude").exists() {
            return parent.to_path_buf();
        }
        current = parent.to_path_buf();
    }
    cwd
}

// =============================================================================
// MyLogger 实例
// =============================================================================

#[derive(Clone)]
pub struct MyLogger {
    caller_name: String,
    pub wfname: String,
    environment: Environment,
    workflow_file: Arc<Mutex<Option<File>>>,
    console_level: LogLevel,
    file_level: LogLevel,
    write_lock: Arc<Mutex<()>>,
    detail_logger_enabled: bool,  // 是否启用 detail 日志（仅开发/测试环境）
}

impl MyLogger {
    /// 创建新的 MyLogger 实例
    ///
    /// # Arguments
    /// * `caller_name` - 调用者类名（作为单例键，也作为日志目录名）
    /// * `log_retention_days` - 日志保留天数
    pub fn new(caller_name: &str, log_retention_days: i32) -> Self {
        let global = get_global_logger();

        // 日志文件名 = caller_name
        let log_name = caller_name;

        // 工作流日志固定写在 logs/{caller_name}/ 下
        let workflow_log_dir = global.get_project_root().join("logs").join(log_name);
        fs::create_dir_all(&workflow_log_dir).ok();

        let workflow_log_path = workflow_log_dir.join(format!("{}.log", log_name));

        if workflow_log_path.exists() && workflow_log_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            let timestamp = Local::now().format("%Y%m%d_%H%M%S");
            let archive_path = workflow_log_dir.join(format!("{}_{}.log", log_name, timestamp));
            fs::rename(&workflow_log_path, &archive_path).ok();
        }

        // 尝试打开工作流日志文件
        let workflow_file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&workflow_log_path)
        {
            Ok(file) => Some(file),
            Err(e) => {
                eprintln!("警告: 无法打开工作流日志文件 {}: {}. 将只使用控制台日志.", workflow_log_path.display(), e);
                None
            }
        };

        let env_str = std::env::var("APP_ENV").unwrap_or_else(|_| "production".to_string());
        let environment: Environment = env_str.parse().unwrap_or(Environment::Production);

        cleanup_workflow_logs(&workflow_log_dir, log_retention_days, log_name);
        global.cleanup_old_logs(log_retention_days);

        Self {
            caller_name: caller_name.to_string(),
            wfname: String::new(),
            environment,
            workflow_file: Arc::new(Mutex::new(workflow_file)),
            console_level: environment.console_level(),
            file_level: environment.file_level(),
            write_lock: Arc::new(Mutex::new(())),
            detail_logger_enabled: environment != Environment::Production,  // 仅开发/测试环境启用
        }
    }

    pub fn log(&self, message: &str, level: LogLevel) {
        let formatted = format!("[{}] {}", self.caller_name, message);
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_name = if self.wfname.is_empty() { &self.caller_name } else { &self.wfname };
        let log_line = format!("{} - {} - {} - {}\n", timestamp, log_name, level.as_str(), formatted);

        let global = get_global_logger();
        if level >= LogLevel::Info {
            global.write_project(&log_line, level);
        }
        // 仅在开发/测试环境写入 detail.log
        if self.detail_logger_enabled {
            global.write_detail(&log_line);
        }

        if level >= self.file_level {
            let _guard = self.write_lock.lock().ok();
            if let Ok(mut file_opt) = self.workflow_file.lock() {
                if let Some(ref mut file) = *file_opt {
                    let _ = file.write_all(log_line.as_bytes());
                }
            }
        }

        if level >= self.console_level {
            print!("{}", log_line);
        }
    }

    pub fn detail(&self, message: &str) { self.log(message, LogLevel::Detail); }
    pub fn debug(&self, message: &str) { self.log(message, LogLevel::Debug); }
    pub fn info(&self, message: &str) { self.log(message, LogLevel::Info); }
    pub fn warn(&self, message: &str) { self.log(message, LogLevel::Warn); }
    pub fn error(&self, message: &str) { self.log(message, LogLevel::Error); }

    /// 获取当前运行环境
    pub fn get_environment(&self) -> Environment {
        self.environment
    }

    /// 设置运行环境（动态切换）
    ///
    /// 会自动更新 console_level、file_level 和 detail_logger_enabled
    pub fn set_environment(&mut self, env: Environment) {
        self.environment = env;
        self.console_level = env.console_level();
        self.file_level = env.file_level();
        // 仅开发/测试环境启用 detail 日志
        self.detail_logger_enabled = env != Environment::Production;
    }

    /// 从环境变量 APP_ENV 设置环境
    pub fn set_environment_from_env(&mut self) {
        let env_str = std::env::var("APP_ENV").unwrap_or_else(|_| "production".to_string());
        let environment: Environment = env_str.parse().unwrap_or(Environment::Production);
        self.set_environment(environment);
    }

    /// 格式化异常信息，包括堆栈跟踪
    ///
    /// # Arguments
    /// * `error` - 任何实现了 std::error::Error trait 的错误
    ///
    /// # Returns
    /// * 格式化的异常信息字符串
    pub fn format_error<E: std::error::Error>(&self, error: &E) -> String {
        let error_type = std::any::type_name::<E>().split("::").last().unwrap_or("Error");
        let error_msg = error.to_string();

        // 获取错误链
        let mut chain = String::new();
        let mut source = error.source();
        while let Some(err) = source {
            chain.push_str(&format!("\n  Caused by: {}", err));
            source = err.source();
        }

        format!("Exception: {}: {}{}", error_type, error_msg, chain)
    }

    /// 记录异常错误日志
    ///
    /// # Arguments
    /// * `error` - 任何实现了 std::error::Error trait 的错误
    /// * `level` - 日志级别，默认为 Error
    pub fn log_error<E: std::error::Error>(&self, error: &E, level: Option<LogLevel>) {
        let message = self.format_error(error);
        self.log(&message, level.unwrap_or(LogLevel::Error));
        // 同时记录详细堆栈到 detail 级别
        self.detail(&format!("Error details: {}", message));
    }

    /// 清空全局 detail.log 文件内容
    pub fn clear_detail_log(&self) {
        let global = get_global_logger();
        let detail_path = global.get_project_root().join("logs").join("project").join("detail.log");
        if detail_path.exists() {
            let _ = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&detail_path);
        }
    }
}

fn cleanup_workflow_logs(logs_dir: &Path, retention_days: i32, dir_name: &str) {
    if retention_days <= 0 { return; }
    let cutoff = Local::now() - Duration::days(retention_days as i64);

    if let Ok(entries) = fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified_dt: DateTime<Local> = modified.into();
                        if modified_dt < cutoff && fs::remove_file(&path).is_ok() {
                            println!("删除过期日志文件: {}/{}", dir_name, path.file_name().unwrap_or_default().to_string_lossy());
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// 单例模式
// =============================================================================

static INSTANCES: OnceLock<RwLock<HashMap<String, Arc<MyLogger>>>> = OnceLock::new();

fn get_instances() -> &'static RwLock<HashMap<String, Arc<MyLogger>>> {
    INSTANCES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 获取或创建 MyLogger 实例（单例模式）
///
/// # Arguments
/// * `caller_name` - 调用者类名（作为单例键，也作为日志目录名）
/// * `log_retention_days` - 日志保留天数（默认3）
pub fn get_logger(caller_name: &str, log_retention_days: i32) -> Arc<MyLogger> {
    let key = caller_name.to_string();

    // 启动周期性日志清理（每小时一次，首次调用时启动）
    GlobalLogger::start_periodic_cleanup(find_project_root(), log_retention_days);

    {
        let instances = get_instances().read();
        if let Some(logger) = instances.get(&key) {
            return logger.clone();
        }
    }

    let mut instances = get_instances().write();
    let logger = Arc::new(MyLogger::new(caller_name, log_retention_days));
    instances.insert(key, logger.clone());
    logger
}

// =============================================================================
// 宏：自动获取调用者类名
// =============================================================================

/// 创建 MyLogger 实例的宏（无参数，禁止传参）
///
/// 唯一用法：mylogger!() - 自动获取类名，retention=3（默认）
///
/// ```rust
/// use std::sync::Arc;
/// use base::mylogger;
///
/// struct MyCapability {
///     logger: Arc<MyLogger>,
/// }
///
/// impl MyCapability {
///     pub fn new() -> Self {
///         Self {
///             logger: mylogger!(),  // 自动获取类名 "MyCapability"
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! mylogger {
    () => {
        $crate::get_logger(
            std::any::type_name::<Self>().split("::").last().unwrap_or("Unknown"),
            3
        )
    };
}

// =============================================================================
// C FFI 导出
// =============================================================================

/// 创建 MyLogger 实例
/// * `caller_name` - 调用者类名
/// * `log_retention_days` - 日志保留天数
///
/// # Safety
/// * `caller_name` 必须是有效的 C 字符串指针或空指针
#[no_mangle]
pub unsafe extern "C" fn mylogger_new(
    caller_name: *const c_char,
    log_retention_days: i32
) -> *mut Arc<MyLogger> {
    let caller_name_str = if caller_name.is_null() {
        "default".to_string()
    } else {
        CStr::from_ptr(caller_name).to_string_lossy().to_string()
    };

    let logger = get_logger(&caller_name_str, log_retention_days);
    Box::into_raw(Box::new(logger))
}

/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle]
pub unsafe extern "C" fn mylogger_log(logger: *const Arc<MyLogger>, level: i32, message: *const c_char) {
    if logger.is_null() || message.is_null() { return; }
    let logger = &*logger;
    let message_str = CStr::from_ptr(message).to_string_lossy().to_string();
    logger.log(&message_str, LogLevel::from_i32(level));
}

/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle] pub unsafe extern "C" fn mylogger_detail(logger: *const Arc<MyLogger>, message: *const c_char) {
    mylogger_log(logger, LogLevel::Detail as i32, message); }
/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle] pub unsafe extern "C" fn mylogger_debug(logger: *const Arc<MyLogger>, message: *const c_char) {
    mylogger_log(logger, LogLevel::Debug as i32, message); }
/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle] pub unsafe extern "C" fn mylogger_info(logger: *const Arc<MyLogger>, message: *const c_char) {
    mylogger_log(logger, LogLevel::Info as i32, message); }
/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle] pub unsafe extern "C" fn mylogger_warn(logger: *const Arc<MyLogger>, message: *const c_char) {
    mylogger_log(logger, LogLevel::Warn as i32, message); }
/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
/// * `message` 必须是有效的 C 字符串指针或空指针
#[no_mangle] pub unsafe extern "C" fn mylogger_error(logger: *const Arc<MyLogger>, message: *const c_char) {
    mylogger_log(logger, LogLevel::Error as i32, message); }

/// # Safety
/// * `logger` 必须是有效的 MyLogger 指针或空指针
#[no_mangle]
pub unsafe extern "C" fn mylogger_free(logger: *mut Arc<MyLogger>) {
    if !logger.is_null() { drop(Box::from_raw(logger)); }
}

/// # Safety
/// * `s` 必须是有效的 C 字符串指针或空指针
#[no_mangle]
pub unsafe extern "C" fn mylogger_free_string(s: *mut c_char) {
    if !s.is_null() { drop(CString::from_raw(s)); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 测试用结构体，演示 mylogger!() 宏的正确用法
    struct TestLogger {
        logger: Arc<MyLogger>,
    }

    impl TestLogger {
        fn new() -> Self {
            Self {
                logger: mylogger!(),  // 自动获取类名 "TestLogger"，retention=3
            }
        }

        fn log_test(&self) {
            self.logger.info("test message from TestLogger");
        }
    }

    #[test]
    fn test_mylogger_macro() {
        let tester = TestLogger::new();
        tester.log_test();
        // 验证 logger 已创建并能正常写入日志
        tester.logger.info("mylogger!() macro test passed");
    }

    #[test]
    fn test_log_level_order() {
        assert!(LogLevel::Detail < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_log_level_from_i32() {
        assert_eq!(LogLevel::from_i32(5), LogLevel::Detail);
        assert_eq!(LogLevel::from_i32(10), LogLevel::Debug);
        assert_eq!(LogLevel::from_i32(20), LogLevel::Info);
        assert_eq!(LogLevel::from_i32(30), LogLevel::Warn);
        assert_eq!(LogLevel::from_i32(40), LogLevel::Error);
        assert_eq!(LogLevel::from_i32(99), LogLevel::Info);
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Detail.as_str(), "DETAIL");
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_environment_from_str() {
        assert_eq!(Environment::from_str_legacy("development"), Environment::Development);
        assert_eq!(Environment::from_str_legacy("test"), Environment::Test);
        assert_eq!(Environment::from_str_legacy("production"), Environment::Production);
    }

    #[test]
    fn test_environment_levels() {
        let prod = Environment::Production;
        assert_eq!(prod.console_level(), LogLevel::Info);
        assert_eq!(prod.file_level(), LogLevel::Info);

        let dev = Environment::Development;
        assert_eq!(dev.console_level(), LogLevel::Detail);
        assert_eq!(dev.file_level(), LogLevel::Detail);
    }

    #[test]
    fn test_logger_levels() {
        // 使用 TestLogger 结构体演示 mylogger!() 用法
        let tester = TestLogger::new();
        tester.logger.detail("detail message");
        tester.logger.debug("debug message");
        tester.logger.info("info message");
        tester.logger.warn("warn message");
        tester.logger.error("error message");
    }

    #[test]
    fn test_get_logger_singleton() {
        // 单例测试需要使用 get_logger（测试函数无 Self 上下文）
        let logger1 = get_logger("singleton_test", 3);
        let logger2 = get_logger("singleton_test", 3);
        assert!(Arc::ptr_eq(&logger1, &logger2));
    }

    #[test]
    fn test_logger_set_environment() {
        // 环境切换测试需要使用 MyLogger::new 创建独立实例（非单例，可修改）
        let mut logger = MyLogger::new("test_env", 3);
        logger.set_environment(Environment::Development);
        assert_eq!(logger.get_environment(), Environment::Development);
    }

    #[test]
    fn test_logger_format_error() {
        use std::io;
        // 错误格式化测试需要使用 get_logger（测试函数无 Self 上下文）
        let logger = get_logger("test_error", 3);
        let error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let formatted = logger.format_error(&error);
        assert!(formatted.contains("Exception"));
        assert!(formatted.contains("file not found"));
    }
}