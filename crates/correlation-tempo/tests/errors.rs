use correlation_core::backend::{BackendError, TelemetryBackend};
use correlation_tempo::TempoClient;
use wiremock::{
    matchers::{path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn empty_on_404() {
    let server = MockServer::start().await;
    Mock::given(path_regex(r"/api/traces/.*"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
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
        .mount(&server)
        .await;
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
        .mount(&server)
        .await;
    let c = TempoClient::new(server.uri());
    let spans = c.fetch_trace("trace1".into()).await.unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].service, "auth");
    assert_eq!(spans[0].operation, "POST /login");
}

#[tokio::test]
async fn search_traces_extracts_hits_with_duration() {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "traces": [
            { "traceID": "aaa", "rootServiceName": "auth", "durationMs": 5 },
            { "traceID": "bbb", "rootServiceName": "transactions", "durationMs": 812 },
            { "traceID": "ccc", "rootServiceName": "notifications" }
        ]
    });
    Mock::given(path("/api/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let c = TempoClient::new(server.uri());
    let hits = c
        .search_traces("{ resource.service.name = \"auth\" }", 0, 100, 5)
        .await
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|h| h.trace_id.as_str()).collect();
    assert_eq!(ids, vec!["aaa", "bbb", "ccc"]);
    assert_eq!(hits[1].duration_ms, 812);
    assert_eq!(hits[1].root_service, "transactions");
    // durationMs omitted by Tempo for ~0ms traces -> defaults to 0.
    assert_eq!(hits[2].duration_ms, 0);
}

#[tokio::test]
async fn search_traces_empty_when_no_matches() {
    let server = MockServer::start().await;
    Mock::given(path("/api/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "traces": [] })))
        .mount(&server)
        .await;
    let c = TempoClient::new(server.uri());
    let hits = c.search_traces("{}", 0, 100, 5).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn search_traces_unreachable_on_error_status() {
    let server = MockServer::start().await;
    Mock::given(path("/api/search"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let c = TempoClient::new(server.uri());
    let res = c.search_traces("{}", 0, 100, 5).await;
    assert!(matches!(res, Err(BackendError::Unreachable)));
}
