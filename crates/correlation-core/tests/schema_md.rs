use correlation_core::schema::{IncidentContext, renderer_md::render_md};

#[test]
fn renders_minimal_incident_to_markdown() {
    let json = include_str!("fixtures/incident_minimal.json");
    let ic: IncidentContext = serde_json::from_str(json).unwrap();
    insta::assert_snapshot!(render_md(&ic));
}
