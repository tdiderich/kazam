use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn generate() -> String {
    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    COUNTER.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    let hash = hasher.finish();
    // 6 hex digits (24 bits, ~16.7M values) - 4 digits (65,536 values) had a
    // real, non-flaky collision risk: 100 IDs into a 16-bit space already
    // carries roughly a 7% birthday-paradox chance of a duplicate, which is
    // exactly what made the uniqueness test intermittently fail.
    format!("kz-{:06x}", hash & 0xFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_across_rapid_calls() {
        let ids: Vec<String> = (0..100).map(|_| generate()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "generated duplicate IDs");
    }

    #[test]
    fn id_format() {
        let id = generate();
        assert!(id.starts_with("kz-"), "bad prefix: {id}");
        assert_eq!(id.len(), 9, "bad length: {id}");
    }
}
