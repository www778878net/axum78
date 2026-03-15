//! axum78 同步服务器
//!
//! 运行: cargo run -p axum78 --bin sync_server

use axum78::sync::create_router;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_path = "tmp/data/remote.db";
    let app = create_router(db_path);

    let addr = "127.0.0.1:3780";
    let listener = TcpListener::bind(addr).await.expect("绑定端口失败");

    tracing::info!("同步服务器启动: http://{}", addr);
    tracing::info!("端点:");
    tracing::info!("  POST /sync/:table      - 上传数据 (protobuf)");
    tracing::info!("  POST /sync/:table/get  - 下载数据 (protobuf)");
    tracing::info!("  GET  /sync/:table/items - 列表数据 (protobuf)");
    tracing::info!("  GET  /health           - 健康检查");

    axum::serve(listener, app).await.expect("服务器启动失败");
}
