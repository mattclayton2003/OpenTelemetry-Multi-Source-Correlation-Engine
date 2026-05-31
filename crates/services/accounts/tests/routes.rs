use accounts::routes;
use axum::http::StatusCode;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use testcontainers::{clients::Cli, images::postgres::Postgres};
use tower::ServiceExt;

async fn setup() -> (axum::Router, testcontainers::Container<'static, Postgres>) {
    static DOCKER: once_cell::sync::Lazy<Cli> = once_cell::sync::Lazy::new(Cli::default);
    let container = DOCKER.run(Postgres::default());
    let port = container.get_host_port_ipv4(5432);
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (routes::router(pool), container)
}

fn body(v: serde_json::Value) -> axum::body::Body {
    axum::body::Body::from(serde_json::to_vec(&v).unwrap())
}

#[tokio::test]
async fn create_then_get_account() {
    let (app, _c) = setup().await;
    let resp = app.clone().oneshot(
        axum::http::Request::builder().method("POST").uri("/accounts")
            .header("content-type","application/json")
            .body(body(json!({"owner":"alice"}))).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let b = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    let resp = app.oneshot(
        axum::http::Request::builder().method("GET").uri(format!("/accounts/{id}"))
            .body(axum::body::Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
