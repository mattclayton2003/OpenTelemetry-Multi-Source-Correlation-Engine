use bank_loadgen::profile::Stage;
use bank_loadgen::runner::run_stage;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn run_stage_fires_expected_count() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let stage = Stage {
        endpoint: format!("POST {}/x", server.uri()),
        rps: 50,
        duration_sec: 1,
        start_offset_sec: None,
        body: None,
    };
    let stats = bank_loadgen::stats::Stats::default();
    run_stage(stage, stats.clone()).await.unwrap();
    // Settle so spawned tasks complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let n = stats
        .current
        .success
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!((30..=70).contains(&n), "expected ~50, got {n}");
}
