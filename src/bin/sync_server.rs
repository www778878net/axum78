//! axum78 同步服务器
//!
//! 运行: cargo run -p axum78 --bin sync_server
//!
//! 同步机制: 上传synclog记录 -> doWork执行实际操作

use axum78::create_router;
use base::ProjectPath;

#[tokio::main]
async fn main() {
    let project = ProjectPath::find().expect("查找项目根目录失败");
    let db_path = project.root().join("crates/axum78/tmp/data/remote.db");
    let db_path = db_path.to_string_lossy().to_string();
    
    let app = create_router(&db_path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3780").await.expect("绑定端口失败");
    
    tracing_subscriber::fmt::init();
    
    tracing::info!("同步服务器启动: http://127.0.0.1:3780");
    tracing::info!("数据库: {}", db_path);
    tracing::info!("端点:");
    tracing::info!("  POST /:apisys/:apimicro/:apiobj/:apifun - 4级路由API");
    tracing::info!("  POST /apitest/testmenu/testtb/get - 下载数据");
    tracing::info!("  POST /apisvc/backsvc/synclog/maddmany - 上传同步记录");
    tracing::info!("  POST /apisvc/backsvc/synclog/dowork - 执行同步操作");
    tracing::info!("  GET  /health - 健康检查");

    axum::serve(listener, app).await.expect("服务器启动失败");
}
