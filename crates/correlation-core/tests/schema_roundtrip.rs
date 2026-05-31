use correlation_core::schema::{IncidentContext, SCHEMA_VERSION};

#[test]
fn round_trip_is_byte_stable() {
    let json = include_str!("fixtures/incident_minimal.json");
    let ic: IncidentContext = serde_json::from_str(json).unwrap();
    let s1 = serde_json::to_string(&ic).unwrap();
    let ic2: IncidentContext = serde_json::from_str(&s1).unwrap();
    let s2 = serde_json::to_string(&ic2).unwrap();
    assert_eq!(s1, s2);
    assert_eq!(ic.schema_version, SCHEMA_VERSION);
}
