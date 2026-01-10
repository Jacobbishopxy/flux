#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    V1,
}

pub const WIRE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct EnvelopePreview {
    pub version: SchemaVersion,
    pub message_type: &'static str,
}

impl EnvelopePreview {
    pub fn new(message_type: &'static str) -> Self {
        Self {
            version: SchemaVersion::V1,
            message_type,
        }
    }

    pub fn topic_hint(&self) -> String {
        format!("{}@{:?}", self.message_type, self.version)
    }
}

pub fn crate_tag() -> &'static str {
    "flux-schema"
}

#[allow(clippy::all)]
#[path = "generated/market_data_generated.rs"]
pub mod market_data_generated;

pub mod fb {
    pub use super::market_data_generated::flux::schema::*;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_hint_formats_version() {
        let env = EnvelopePreview::new("tick");
        assert_eq!(env.topic_hint(), "tick@V1");
    }

    #[test]
    fn envelope_roundtrips_health_response() {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let msg = fbb.create_string("ready");
        let health = fb::HealthResponse::create(
            &mut fbb,
            &fb::HealthResponseArgs {
                ok: true,
                message: Some(msg),
            },
        );
        let envelope = fb::Envelope::create(
            &mut fbb,
            &fb::EnvelopeArgs {
                schema_version: WIRE_SCHEMA_VERSION,
                type_hint: fb::MessageType::HEALTH_RESPONSE,
                correlation_id: None,
                message_type: fb::Message::HealthResponse,
                message: Some(health.as_union_value()),
            },
        );
        fb::finish_envelope_buffer(&mut fbb, envelope);

        let parsed = fb::root_as_envelope(fbb.finished_data()).unwrap();
        assert_eq!(parsed.schema_version(), WIRE_SCHEMA_VERSION);
        assert_eq!(parsed.type_hint(), fb::MessageType::HEALTH_RESPONSE);
        assert_eq!(parsed.message_type(), fb::Message::HealthResponse);

        let parsed_health = parsed.message_as_health_response().unwrap();
        assert!(parsed_health.ok());
        assert_eq!(parsed_health.message(), Some("ready"));
    }
}
