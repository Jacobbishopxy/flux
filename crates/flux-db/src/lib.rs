use flux_core::{Symbol, TimeRange};
use flux_schema::SchemaVersion;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub uri: String,
    pub app_name: String,
}

impl DbConfig {
    pub fn new(uri: impl Into<String>, app_name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            app_name: app_name.into(),
        }
    }
}

pub fn connection_string(cfg: &DbConfig) -> String {
    format!("{}?application_name={}", cfg.uri, cfg.app_name)
}

pub fn plan_tick_window(symbol: &Symbol, range: &TimeRange) -> String {
    format!(
        "ticks for {} from {} to {} (v{:?})",
        symbol.0,
        range.start_epoch,
        range.end_epoch,
        SchemaVersion::V1
    )
}

pub fn crate_tag() -> &'static str {
    "flux-db"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_string_adds_app_name() {
        let cfg = DbConfig::new("postgres://localhost/db", "flux-test");
        let conn = connection_string(&cfg);
        assert!(conn.contains("application_name=flux-test"));
    }

    #[test]
    fn planner_mentions_symbol() {
        let plan = plan_tick_window(&Symbol::from("AAPL"), &TimeRange::new(0, 10));
        assert!(plan.contains("AAPL"));
    }
}
