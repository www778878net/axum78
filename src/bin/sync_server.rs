//! axum78 同步服务器
//!
//! 运行: cargo run -p axum78 --bin sync_server (需要配 workspace)
//! 或: cd crates/axum78 && cargo run --bin sync_server
//!
//! 同步机制: 上传datasync记录 -> doWork执行实际操作

use axum78::create_router;

#[tokio::main]
async fn main() {
    let app = create_router();

    let port = std::env::var("PORT").unwrap_or_else(|_| "8686".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("绑定端口失败");

    tracing_subscriber::fmt::init();

    tracing::info!("同步服务器启动: http://{}", addr);
    tracing::info!("端点:");
    tracing::info!("  POST /:apisys/:apimicro/:apiobj/:apifun - 4级路由API");
    tracing::info!("  POST /apisvc/backsvc/datasync/maddmany - 上传同步记录");
    tracing::info!("  POST /apisvc/backsvc/datasync/dowork - 执行同步操作");
    tracing::info!("  POST /apisvc/backsvc/datasync/get - 查询同步记录");

    axum::serve(listener, app).await.expect("服务器启动失败");
}
