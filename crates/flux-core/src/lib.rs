#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol(pub String);

impl From<&str> for Symbol {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub start_epoch: i64,
    pub end_epoch: i64,
}

impl TimeRange {
    pub fn new(start_epoch: i64, end_epoch: i64) -> Self {
        Self {
            start_epoch,
            end_epoch,
        }
    }

    pub fn duration_seconds(&self) -> Option<i64> {
        (self.end_epoch >= self.start_epoch).then_some(self.end_epoch - self.start_epoch)
    }
}

pub fn crate_tag() -> &'static str {
    "flux-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_from_str() {
        let symbol = Symbol::from("AAPL");
        assert_eq!(symbol.0, "AAPL");
    }

    #[test]
    fn duration_is_positive() {
        let range = TimeRange::new(1, 4);
        assert_eq!(range.duration_seconds(), Some(3));
    }
}
