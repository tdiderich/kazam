//! Anchor slug generation for `section` / `header` components.
//!
//! Two guarantees:
//!
//! 1. `slugify(text)` is deterministic: lowercase, ASCII-only, hyphens for
//!    whitespace, punctuation stripped. Emoji and any non-alphanumeric
//!    non-ASCII chars are dropped without collapsing the surrounding words
//!    (so "⚡ Move at Machine Speed" → "move-at-machine-speed").
//!
//! 2. [`Tracker`] dedupes within one page - the second instance of a slug
//!    becomes `slug-2`, the third `slug-3`, etc. The tracker is reset at
//!    the start of each page render via [`reset`], keyed on a thread-local
//!    so the rendering call chain doesn't need to plumb state through
//!    every `Rendered` boundary.

use std::cell::RefCell;
use std::collections::HashMap;

pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_dash = true; // start in "just emitted dash" state so leading junk is eaten
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
        // Everything else (punctuation, emoji, non-ASCII letters) is
        // dropped - but without emitting a dash, so "Q3/Q4" collapses to
        // "q3q4" rather than "q3-q4". Adjust if a real case surfaces.
    }
    // Strip a trailing dash if the input ended on punctuation/whitespace.
    while out.ends_with('-') {
        out.pop();
    }
    out
}

thread_local! {
    static TRACKER: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

/// Clear the per-page slug tracker. Call at the top of each page render so
/// that collision suffixes don't leak across pages in the same build.
pub fn reset() {
    TRACKER.with(|t| t.borrow_mut().clear());
}

/// Resolve the rendered id for a component.
///
/// - `explicit` wins when set and non-empty (trimmed).
/// - Otherwise `fallback` gets slugified.
/// - Returns `None` if the result would be empty.
///
/// Either way, the final string is deduped against previously-issued ids
/// on this page: a second `"outcomes"` becomes `"outcomes-2"`.
pub fn resolve(explicit: Option<&str>, fallback: Option<&str>) -> Option<String> {
    let candidate = match explicit.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => slugify(id),
        None => fallback.map(slugify).unwrap_or_default(),
    };
    if candidate.is_empty() {
        return None;
    }
    Some(dedupe(candidate))
}

fn dedupe(base: String) -> String {
    TRACKER.with(|t| {
        let mut map = t.borrow_mut();
        let count = map.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{}-{}", base, count)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh<T>(f: impl FnOnce() -> T) -> T {
        reset();
        f()
    }

    #[test]
    fn lowercases_and_hyphens_whitespace() {
        assert_eq!(slugify("Success Outcomes"), "success-outcomes");
    }

    #[test]
    fn strips_punctuation_and_emoji() {
        assert_eq!(slugify("⚡ Move at Machine Speed"), "move-at-machine-speed");
        assert_eq!(slugify("What's next?"), "whats-next");
    }

    #[test]
    fn collapses_consecutive_separators() {
        assert_eq!(slugify("foo   bar"), "foo-bar");
        assert_eq!(slugify("foo - bar"), "foo-bar");
        assert_eq!(slugify("foo__bar"), "foo-bar");
    }

    #[test]
    fn trims_trailing_dash_from_punctuation_suffix() {
        assert_eq!(slugify("Questions?"), "questions");
        assert_eq!(slugify("Hi!   "), "hi");
    }

    #[test]
    fn resolve_prefers_explicit_over_fallback() {
        fresh(|| {
            assert_eq!(
                resolve(Some("outcomes"), Some("Success outcomes")),
                Some("outcomes".into())
            );
        });
    }

    #[test]
    fn resolve_falls_back_to_heading_slug() {
        fresh(|| {
            assert_eq!(
                resolve(None, Some("Success outcomes")),
                Some("success-outcomes".into())
            );
        });
    }

    #[test]
    fn resolve_returns_none_when_empty() {
        fresh(|| {
            assert_eq!(resolve(None, None), None);
            assert_eq!(resolve(Some(""), None), None);
            assert_eq!(resolve(Some("   "), Some("")), None);
            assert_eq!(resolve(None, Some("⚡")), None);
        });
    }

    #[test]
    fn collision_suffix_increments() {
        fresh(|| {
            assert_eq!(resolve(None, Some("Outcomes")), Some("outcomes".into()));
            assert_eq!(resolve(None, Some("Outcomes")), Some("outcomes-2".into()));
            assert_eq!(resolve(None, Some("Outcomes")), Some("outcomes-3".into()));
        });
    }

    #[test]
    fn reset_clears_state_between_pages() {
        reset();
        assert_eq!(resolve(None, Some("X")), Some("x".into()));
        reset();
        assert_eq!(resolve(None, Some("X")), Some("x".into()));
    }
}
