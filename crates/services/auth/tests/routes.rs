use auth::routes;
use axum::http::StatusCode;
use serde_json::json;
use tower::ServiceExt;

fn body(json: serde_json::Value) -> axum::body::Body {
    axum::body::Body::from(serde_json::to_vec(&json).unwrap())
}

#[tokio::test]
async fn login_returns_valid_jwt() {
    let app = routes::router();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(body(json!({"user":"alice","password":"pw"})))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["token"].as_str().unwrap().split('.').count() == 3);
}

#[tokio::test]
async fn verify_round_trips_login_token() {
    let app = routes::router();
    let token = {
        let r = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header("content-type", "application/json")
                    .body(body(json!({"user":"alice","password":"pw"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        let b = axum::body::to_bytes(r.into_body(), 8192).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        v["token"].as_str().unwrap().to_string()
    };
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/auth/verify")
                .header("content-type", "application/json")
                .body(body(json!({"token":token})))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
