use axum::http::StatusCode;
use bank_common::health::router;
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_200() {
    let app = router();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_200_when_no_checks_registered() {
    let app = router();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/ready")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
