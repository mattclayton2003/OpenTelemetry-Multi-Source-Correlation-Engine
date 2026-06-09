use chrono::Utc;
use correlation_core::backend::{AnomalyWindowQuery, BackendError, MetricQuery, TelemetryBackend};
use correlation_prom::PromClient;
use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

fn mq() -> MetricQuery {
    MetricQuery {
        metric: "http_request_duration_seconds:p99".into(),
        service: "auth".into(),
        start: Utc::now() - chrono::Duration::seconds(60),
        end: Utc::now(),
    }
}

fn aq() -> AnomalyWindowQuery {
    AnomalyWindowQuery {
        metric: "http_request_duration_seconds:p99".into(),
        start: Utc::now() - chrono::Duration::seconds(60),
        end: Utc::now(),
    }
}

#[tokio::test]
async fn unreachable_when_server_down() {
    let c = PromClient::new("http://127.0.0.1:1".into());
    let res = c.fetch_metric_series(mq()).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}

#[tokio::test]
async fn unreachable_on_5xx() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v1/query_range"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let c = PromClient::new(server.uri());
    let res = c.fetch_metric_series(mq()).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}

#[tokio::test]
async fn parses_minimal_series_response() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "data": {
            "result": [{
                "metric": {},
                "values": [[1716465600.0, "0.025"], [1716465605.0, "0.030"]]
            }]
        }
    });
    Mock::given(path("/api/v1/query_range"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let c = PromClient::new(server.uri());
    let series = c.fetch_metric_series(mq()).await.unwrap();
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].points.len(), 2);
}

#[tokio::test]
async fn parses_minimal_anomaly_window_response() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "data": {
            "result": [{
                "metric": { "service": "transactions" },
                "values": [[1716465600.0, "1.5"]]
            }]
        }
    });
    Mock::given(path("/api/v1/query_range"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let c = PromClient::new(server.uri());
    let points = c.query_metric_window(aq()).await.unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].service, "transactions");
    assert_eq!(points[0].value, 1.5);
}
