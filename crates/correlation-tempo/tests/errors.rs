use correlation_tempo::TempoClient;
use correlation_core::backend::{TelemetryBackend, BackendError};
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::path_regex};

#[tokio::test]
async fn empty_on_404() {
    let server = MockServer::start().await;
    Mock::given(path_regex(r"/api/traces/.*"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;
    let c = TempoClient::new(server.uri());
    let res = c.fetch_trace("abc".into()).await;
    assert!(matches!(res, Err(BackendError::Empty)));
}

#[tokio::test]
async fn unreachable_when_server_down() {
    let c = TempoClient::new("http://127.0.0.1:1".into());
    let res = c.fetch_trace("abc".into()).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}

#[tokio::test]
async fn unreachable_on_garbage_json() {
    let server = MockServer::start().await;
    Mock::given(path_regex(r"/api/traces/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
        .mount(&server).await;
    let c = TempoClient::new(server.uri());
    let res = c.fetch_trace("abc".into()).await;
    assert!(matches!(res, Err(BackendError::Unreachable))); // RetryPolicy maps parse failures here
}

#[tokio::test]
async fn parses_minimal_otlp_response() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "batches": [{
            "resource": { "attributes": [{ "key": "service.name", "value": { "stringValue": "auth" } }] },
            "scopeSpans": [{ "spans": [{
                "spanId": "span1", "traceId": "trace1", "parentSpanId": "",
                "name": "POST /login",
                "startTimeUnixNano": "1716465600000000000",
                "endTimeUnixNano": "1716465600050000000",
                "status": { "code": 0 }
            }]}]
        }]
    });
    Mock::given(path_regex(r"/api/traces/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server).await;
    let c = TempoClient::new(server.uri());
    let spans = c.fetch_trace("trace1".into()).await.unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].service, "auth");
    assert_eq!(spans[0].operation, "POST /login");
}
