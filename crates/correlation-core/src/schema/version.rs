pub const SCHEMA_VERSION: &str = "1.0.0";

pub fn major_compatible(version: &str) -> bool {
    version.split('.').next() == Some("1")
}
