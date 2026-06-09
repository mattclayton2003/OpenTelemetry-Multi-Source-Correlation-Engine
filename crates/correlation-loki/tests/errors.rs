use chrono::Utc;
use correlation_core::backend::{BackendError, LogQuery, TelemetryBackend};
use correlation_loki::LokiClient;
use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

fn q(services: Vec<String>) -> LogQuery {
    LogQuery {
        services,
        start: Utc::now() - chrono::Duration::seconds(60),
        end: Utc::now(),
        level_at_least: None,
    }
}

#[tokio::test]
async fn empty_services_returns_empty() {
    let c = LokiClient::new("http://127.0.0.1:1".into());
    let res = c.fetch_logs(q(vec![])).await.unwrap();
    assert!(res.is_empty());
}

#[tokio::test]
async fn unreachable_when_server_down() {
    let c = LokiClient::new("http://127.0.0.1:1".into());
    let res = c.fetch_logs(q(vec!["auth".into()])).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}

#[tokio::test]
async fn unreachable_on_5xx() {
    let server = MockServer::start().await;
    Mock::given(path("/loki/api/v1/query_range"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let c = LokiClient::new(server.uri());
    let res = c.fetch_logs(q(vec!["auth".into()])).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}

#[tokio::test]
async fn parses_minimal_loki_response() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "data": {
            "result": [{
                "stream": { "service_name": "auth", "level": "ERROR" },
                "values": [["1716465600000000000", "smtp send failed: i/o timeout"]]
            }]
        }
    });
    Mock::given(path("/loki/api/v1/query_range"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let c = LokiClient::new(server.uri());
    let logs = c.fetch_logs(q(vec!["auth".into()])).await.unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].service, "auth");
    assert_eq!(logs[0].level, "ERROR");
}
