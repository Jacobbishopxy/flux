#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    V1,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_hint_formats_version() {
        let env = EnvelopePreview::new("tick");
        assert_eq!(env.topic_hint(), "tick@V1");
    }
}
