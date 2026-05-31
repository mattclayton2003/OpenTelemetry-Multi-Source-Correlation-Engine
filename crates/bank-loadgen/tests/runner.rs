use bank_loadgen::runner::run_stage;
use bank_loadgen::profile::Stage;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::method};

#[tokio::test]
async fn run_stage_fires_expected_count() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200)).mount(&server).await;
    let stage = Stage {
        endpoint: format!("POST {}/x", server.uri()),
        rps: 50, duration_sec: 1, start_offset_sec: None, body: None,
    };
    let stats = bank_loadgen::stats::Stats::default();
    run_stage(stage, stats.clone()).await.unwrap();
    // Settle so spawned tasks complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let n = stats.current.success.load(std::sync::atomic::Ordering::SeqCst);
    assert!(n >= 30 && n <= 70, "expected ~50, got {n}");
}
