//! axum78 同步服务器 - 精简版
//!
//! 绕过复杂路由系统，直接挂载 datasync/datasync_mysql handler
//!
//! 运行: cargo run --bin sync_server

use axum::{
    Router,
    routing::post,
    extract::Path,
    response::IntoResponse,
    body::Bytes,
    http::StatusCode,
};
use base::UpInfo;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/apisvc/backsvc/datasync/:apifun", post(datasync_handler))
        .route("/apisvc/backsvc/datasync_mysql/:apifun", post(datasync_mysql_handler));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8686".to_string());
    let addr = format!("0.0.0.0:{}", port);

    tracing_subscriber::fmt::init();
    tracing::info!("同步服务器启动: http://{}", addr);
    tracing::info!("  POST /apisvc/backsvc/datasync/:apifun");
    tracing::info!("  POST /apisvc/backsvc/datasync_mysql/:apifun");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("绑定端口失败");
    axum::serve(listener, app).await.expect("服务器启动失败");
}

/// datasync (SQLite) handler
async fn datasync_handler(
    Path(apifun): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    let up: UpInfo = match serde_json::from_slice(&body) {
        Ok(u) => u,
        Err(e) => {
            let resp = base::Response::fail(&format!("解析失败: {}", e), -1);
            let json = serde_json::to_string(&resp).unwrap_or_default();
            return (StatusCode::BAD_REQUEST, [(axum::http::header::CONTENT_TYPE, "application/json")], Bytes::from(json));
        }
    };
    let (status, resp_body) = crate::apisvc::backsvc::datasync::handle(&apifun, up).await;
    (status, [(axum::http::header::CONTENT_TYPE, "application/json")], resp_body)
}

/// datasync_mysql (MySQL) handler
async fn datasync_mysql_handler(
    Path(apifun): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    let up: UpInfo = match serde_json::from_slice(&body) {
        Ok(u) => u,
        Err(e) => {
            let resp = base::Response::fail(&format!("解析失败: {}", e), -1);
            let json = serde_json::to_string(&resp).unwrap_or_default();
            return (StatusCode::BAD_REQUEST, [(axum::http::header::CONTENT_TYPE, "application/json")], Bytes::from(json));
        }
    };
    // datasync_mysql::handle 需要 VerifyResult，从 up 构造（中间件原本会做这件事）
    let verify_result = crate::VerifyResult {
        cid: up.cid.clone(),
        uid: up.uid.clone(),
        uname: up.uname.clone(),
    };
    let (status, resp_body) = crate::apisvc::backsvc::datasync_mysql::handle(&apifun, up, &verify_result).await;
    (status, [(axum::http::header::CONTENT_TYPE, "application/json")], resp_body)
}
