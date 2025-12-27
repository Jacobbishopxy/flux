use flux_core::{Symbol, TimeRange};
use flux_schema::{EnvelopePreview, SchemaVersion};

#[derive(Debug, Clone)]
pub struct ZmqEndpoint {
    pub url: String,
    pub topic: String,
}

impl ZmqEndpoint {
    pub fn new(url: impl Into<String>, topic: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            topic: topic.into(),
        }
    }
}

pub fn subscription_label(symbol: &Symbol, endpoint: &ZmqEndpoint) -> String {
    format!("sub:{}:{} -> {}", symbol.0, endpoint.topic, endpoint.url)
}

pub fn backfill_envelope(range: TimeRange) -> EnvelopePreview {
    let message_type = if range.duration_seconds().unwrap_or_default() > 0 {
        "chunked-range"
    } else {
        "heartbeat"
    };
    EnvelopePreview {
        version: SchemaVersion::V1,
        message_type,
    }
}

pub fn crate_tag() -> &'static str {
    "flux-io"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_includes_topic() {
        let endpoint = ZmqEndpoint::new("tcp://localhost:5555", "market.feed");
        let symbol = Symbol::from("ES");
        assert!(subscription_label(&symbol, &endpoint).contains("market.feed"));
    }
}
