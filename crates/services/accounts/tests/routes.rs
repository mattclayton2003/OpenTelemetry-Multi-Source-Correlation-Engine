use accounts::routes;
use axum::http::StatusCode;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tower::ServiceExt;

async fn setup() -> (axum::Router, ContainerAsync<GenericImage>) {
    let container = GenericImage::new("postgres", "16")
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "postgres")
        .start()
        .await
        .expect("start postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // The "ready" log line can appear during bootstrap before the server is
    // actually accepting connections, so retry the initial connection.
    let pool = connect_with_retry(&url).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (routes::router(pool), container)
}

async fn connect_with_retry(url: &str) -> PgPool {
    let mut last_err = None;
    for _ in 0..40 {
        match PgPoolOptions::new().max_connections(2).connect(url).await {
            Ok(pool) => return pool,
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    panic!("postgres never became ready: {last_err:?}");
}

fn body(v: serde_json::Value) -> axum::body::Body {
    axum::body::Body::from(serde_json::to_vec(&v).unwrap())
}

#[tokio::test]
async fn create_then_get_account() {
    let (app, _c) = setup().await;
    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/accounts")
                .header("content-type", "application/json")
                .body(body(json!({"owner":"alice"})))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let b = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/accounts/{id}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
