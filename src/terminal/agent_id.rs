use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable identity of an agent, kept with its terminal across sessions, pane
/// moves and server restarts.
///
/// Ids start with a digit so they never collide with agent names, which must
/// start with a lowercase letter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AgentId(String);

static NEXT_AGENT_ID: AtomicU64 = AtomicU64::new(1);

impl AgentId {
    pub fn alloc() -> Self {
        use sha2::{Digest, Sha256};

        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_micros())
            .unwrap_or(0);
        let counter = NEXT_AGENT_ID.fetch_add(1, Ordering::Relaxed);
        let digest = Sha256::digest(format!("{micros}:{counter}:{}", std::process::id()));
        let lead = digest[0] % 10;
        let rest = u32::from_be_bytes([digest[1], digest[2], digest[3], digest[4]]) & 0x0fff_ffff;
        Self(format!("{lead}{rest:07x}"))
    }

    /// Accepts the shape `alloc` produces: a digit, then seven lowercase hex digits.
    pub fn parse(value: &str) -> Option<Self> {
        let mut chars = value.chars();
        let valid = value.len() == 8
            && chars.next().is_some_and(|ch| ch.is_ascii_digit())
            && chars.all(|ch| matches!(ch, '0'..='9' | 'a'..='f'));
        valid.then(|| Self(value.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocated_ids_parse_and_never_look_like_agent_names() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = AgentId::alloc();
            assert_eq!(AgentId::parse(id.as_str()), Some(id.clone()));
            assert!(id.as_str().starts_with(|ch: char| ch.is_ascii_digit()));
            seen.insert(id);
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn parse_rejects_other_shapes() {
        for value in [
            "",
            "a1234567",
            "1234567",
            "123456789",
            "1234567G",
            "1234567A",
        ] {
            assert_eq!(AgentId::parse(value), None, "{value:?}");
        }
    }
}
