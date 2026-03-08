//! 服务器启动

use axum::Router;
use std::net::SocketAddr;
use tracing::info;

/// 服务器配置
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}

/// 启动服务器
pub async fn start_server(router: Router<()>, config: ServerConfig) {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid address");

    info!("🚀 服务器启动: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, router)
        .await
        .expect("Server failed");
}

/// 服务器构建器
pub struct Server {
    router: Router<()>,
    config: ServerConfig,
}

impl Server {
    /// 创建新服务器
    pub fn new(router: Router<()>) -> Self {
        Self {
            router,
            config: ServerConfig::default(),
        }
    }

    /// 设置端口
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// 设置主机
    pub fn host(mut self, host: &str) -> Self {
        self.config.host = host.to_string();
        self
    }

    /// 启动服务器
    pub async fn run(self) {
        start_server(self.router, self.config).await;
    }
}