//! Freshness metadata: staleness check + reporting.
//!
//! Staleness is computed at build time — zero runtime JS. A page is stale
//! when `updated + review_every < today`. "Today" comes from the env var
//! `KAZAM_TODAY` when set (deterministic tests), otherwise from the
//! system clock.
//!
//! Date handling is hand-rolled against ISO `YYYY-MM-DD`. We only care
//! about day-resolution comparisons, so days-since-1970-01-01 via the
//! Julian-day-number algorithm is enough — no chrono / time dep.
//!
//! Duration parsing accepts `Nd` / `Nw` / `Nm` / `Ny` and the word
//! shortcuts `weekly` / `monthly` / `quarterly` / `yearly` / `annually`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::{Freshness, FreshnessValue};

/// Number of days before the review deadline at which a page starts
/// surfacing a yellow "review due soon" banner. Inside this window the
/// page is not yet overdue but reviewers should see the nudge.
pub const DUE_SOON_WINDOW_DAYS: i64 = 7;

/// A page's current freshness state. The renderer picks a banner variant
/// (and the build report picks a tone) based on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessStatus {
    /// No banner. Either no freshness metadata, or comfortably inside the
    /// review window (more than `DUE_SOON_WINDOW_DAYS` to go).
    Fresh,
    /// Yellow banner — review comes due within `DUE_SOON_WINDOW_DAYS`.
    /// `days_until_due` is non-negative.
    DueSoon { days_until_due: i64 },
    /// Red banner — review window has elapsed. `days_overdue` is positive.
    Overdue { days_overdue: i64 },
    /// Page has passed its hard expiration date. Stronger than Overdue —
    /// the content is no longer relevant, not just due for review.
    Expired { days_past_expiry: i64 },
}

/// Today's date as `YYYY-MM-DD`. Honors `KAZAM_TODAY` for deterministic
/// tests, else reads the system clock.
pub fn today_iso() -> String {
    if let Ok(s) = std::env::var("KAZAM_TODAY") {
        if !s.is_empty() {
            return s;
        }
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    iso_from_days_since_epoch(days)
}

/// Structured days-since-epoch info for a page's freshness metadata.
pub struct FreshnessInfo {
    pub updated_days: Option<i64>,
    pub review_days: Option<i64>,
    pub expires_days: Option<i64>,
    pub today_days: i64,
}

impl FreshnessInfo {
    pub fn days_since_update(&self) -> Option<i64> {
        self.updated_days.map(|u| self.today_days - u)
    }

    /// True when an `updated` date AND a `review_every` cadence are set AND
    /// the elapsed days exceed the cadence. Pages without both fields set
    /// are never "stale" — they simply have nothing to compare against.
    #[allow(dead_code)]
    pub fn is_stale(&self) -> bool {
        matches!(
            self.status(),
            FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. }
        )
    }

    /// Freshness state. Expired takes priority over everything else.
    pub fn status(&self) -> FreshnessStatus {
        if let Some(exp) = self.expires_days {
            if self.today_days > exp {
                return FreshnessStatus::Expired {
                    days_past_expiry: self.today_days - exp,
                };
            }
        }
        let (elapsed, cadence) = match (self.days_since_update(), self.review_days) {
            (Some(e), Some(c)) => (e, c),
            _ => return FreshnessStatus::Fresh,
        };
        let days_until_due = cadence - elapsed;
        if days_until_due < 0 {
            FreshnessStatus::Overdue {
                days_overdue: -days_until_due,
            }
        } else if days_until_due <= DUE_SOON_WINDOW_DAYS {
            FreshnessStatus::DueSoon { days_until_due }
        } else {
            FreshnessStatus::Fresh
        }
    }
}

/// Parse a `Freshness` struct into days-since-epoch integers relative to
/// `today_iso` (a `YYYY-MM-DD` string). Returns `None` when there's no
/// freshness metadata at all, or when the value is `FreshnessValue::Never`.
pub fn info_for(f: Option<&Freshness>, today_iso: &str) -> Option<FreshnessInfo> {
    let f = f?;
    let today_days = parse_iso_date(today_iso).unwrap_or(0);
    let updated_days = f.updated.as_deref().and_then(parse_iso_date);
    let expires_days = f.expires.as_deref().and_then(parse_iso_date);
    let review_days = f.review_every.as_deref().and_then(parse_duration_days);
    Some(FreshnessInfo {
        updated_days,
        review_days,
        expires_days,
        today_days,
    })
}

/// Extract the inner `Freshness` from an `Option<FreshnessValue>`, or
/// `None` if the value is absent or is the "never" variant.
pub fn freshness_struct(fv: Option<&FreshnessValue>) -> Option<&Freshness> {
    fv?.as_full()
}

// ── `kazam freshness` command ──────────────────────────────────────────────

/// Run the `kazam freshness` command — walk `dir`, evaluate staleness for
/// every page, and print a JSON or human-readable report.
pub fn run_command(dir: &Path, pretty: bool, threshold: Option<u64>) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::fs;
    use walkdir::WalkDir;

    let today = today_iso();

    #[derive(Debug)]
    struct PageResult {
        path: String,
        title: String,
        status: FreshnessStatus,
        days_overdue: Option<i64>,
        #[allow(dead_code)]
        days_until_due: Option<i64>,
        owner: Option<String>,
        updated: Option<String>,
        review_every: Option<String>,
        is_never: bool,
        no_freshness: bool,
    }

    let mut results: Vec<PageResult> = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 && e.file_name() == "_site" && e.file_type().is_dir() {
                return false;
            }
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        })
    {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" || fname == "404.yaml" {
            continue;
        }

        let is_yaml = path.extension().map(|e| e == "yaml").unwrap_or(false);
        if !is_yaml {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let content = fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let page: crate::types::Page =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

        let display_path = rel_str;

        match page.freshness.as_ref() {
            None => {
                // No freshness metadata at all
                results.push(PageResult {
                    path: display_path,
                    title: page.title,
                    status: FreshnessStatus::Fresh,
                    days_overdue: None,
                    days_until_due: None,
                    owner: None,
                    updated: None,
                    review_every: None,
                    is_never: false,
                    no_freshness: true,
                });
            }
            Some(fv) if fv.is_never() => {
                results.push(PageResult {
                    path: display_path,
                    title: page.title,
                    status: FreshnessStatus::Fresh,
                    days_overdue: None,
                    days_until_due: None,
                    owner: None,
                    updated: None,
                    review_every: None,
                    is_never: true,
                    no_freshness: false,
                });
            }
            Some(fv) => {
                let f = fv.as_full().unwrap();
                // Apply threshold override if provided
                let effective_review_every = if let Some(days) = threshold {
                    Some(format!("{}d", days))
                } else {
                    f.review_every.clone()
                };

                let f_override = Freshness {
                    updated: f.updated.clone(),
                    review_every: effective_review_every,
                    owner: f.owner.clone(),
                    sources_of_truth: f.sources_of_truth.clone(),
                    expires: f.expires.clone(),
                };

                let status = match info_for(Some(&f_override), &today) {
                    Some(info) => info.status(),
                    None => FreshnessStatus::Fresh,
                };

                let (days_overdue, days_until_due) = match status {
                    FreshnessStatus::Expired { days_past_expiry } => (Some(days_past_expiry), None),
                    FreshnessStatus::Overdue { days_overdue } => (Some(days_overdue), None),
                    FreshnessStatus::DueSoon { days_until_due } => (None, Some(days_until_due)),
                    FreshnessStatus::Fresh => (None, None),
                };

                results.push(PageResult {
                    path: display_path,
                    title: page.title,
                    status,
                    days_overdue,
                    days_until_due,
                    owner: f.owner.clone(),
                    updated: f.updated.clone(),
                    review_every: f.review_every.clone(),
                    is_never: false,
                    no_freshness: false,
                });
            }
        }
    }

    // Compute summary counts
    let total = results.len();
    let fresh_count = results
        .iter()
        .filter(|r| {
            matches!(r.status, FreshnessStatus::Fresh) && !r.is_never && !r.no_freshness
        })
        .count();
    let due_soon_count = results
        .iter()
        .filter(|r| matches!(r.status, FreshnessStatus::DueSoon { .. }))
        .count();
    let overdue_count = results
        .iter()
        .filter(|r| matches!(r.status, FreshnessStatus::Overdue { .. }))
        .count();
    let expired_count = results
        .iter()
        .filter(|r| matches!(r.status, FreshnessStatus::Expired { .. }))
        .count();
    let never_count = results.iter().filter(|r| r.is_never).count();
    let no_freshness_count = results.iter().filter(|r| r.no_freshness).count();

    // Non-fresh pages only in the `pages` array
    let non_fresh: Vec<&PageResult> = results
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                FreshnessStatus::DueSoon { .. }
                    | FreshnessStatus::Overdue { .. }
                    | FreshnessStatus::Expired { .. }
            )
        })
        .collect();

    if pretty {
        // Human-readable table output
        println!("Freshness report — {}", today);
        println!(
            "  total: {}  fresh: {}  due_soon: {}  overdue: {}  expired: {}  never: {}  no_freshness: {}",
            total, fresh_count, due_soon_count, overdue_count, expired_count, never_count, no_freshness_count
        );

        if non_fresh.is_empty() {
            println!("\nAll pages are fresh.");
        } else {
            println!();
            let mut sorted: Vec<&PageResult> = non_fresh;
            sorted.sort_by(|a, b| {
                let a_overdue = a.days_overdue.unwrap_or(-1);
                let b_overdue = b.days_overdue.unwrap_or(-1);
                b_overdue.cmp(&a_overdue)
            });
            for r in &sorted {
                let status_str = match r.status {
                    FreshnessStatus::Expired { days_past_expiry } => {
                        format!("EXPIRED ({} days)", days_past_expiry)
                    }
                    FreshnessStatus::Overdue { days_overdue } => {
                        format!("OVERDUE ({} days)", days_overdue)
                    }
                    FreshnessStatus::DueSoon { days_until_due } => {
                        format!("DUE SOON (in {} days)", days_until_due)
                    }
                    FreshnessStatus::Fresh => "FRESH".to_string(),
                };
                let owner = r
                    .owner
                    .as_deref()
                    .map(|o| format!("  owner: {}", o))
                    .unwrap_or_default();
                println!("  [{status}] {path}{owner}", status = status_str, path = r.path);
            }
        }
    } else {
        // JSON output
        let mut pages_json = String::from("[\n");
        for (i, r) in non_fresh.iter().enumerate() {
            let status_str = match r.status {
                FreshnessStatus::Expired { .. } => "expired",
                FreshnessStatus::Overdue { .. } => "overdue",
                FreshnessStatus::DueSoon { .. } => "due_soon",
                FreshnessStatus::Fresh => "fresh",
            };
            let days_overdue_str = r
                .days_overdue
                .map(|d| d.to_string())
                .unwrap_or_else(|| "null".to_string());
            let owner_str = r
                .owner
                .as_deref()
                .map(|o| format!("\"{}\"", json_escape(o)))
                .unwrap_or_else(|| "null".to_string());
            let updated_str = r
                .updated
                .as_deref()
                .map(|u| format!("\"{}\"", json_escape(u)))
                .unwrap_or_else(|| "null".to_string());
            let review_every_str = r
                .review_every
                .as_deref()
                .map(|rv| format!("\"{}\"", json_escape(rv)))
                .unwrap_or_else(|| "null".to_string());

            let comma = if i + 1 < non_fresh.len() { "," } else { "" };
            pages_json.push_str(&format!(
                "    {{\"path\":\"{}\",\"title\":\"{}\",\"status\":\"{}\",\"days_overdue\":{},\"owner\":{},\"updated\":{},\"review_every\":{}}}{}\n",
                json_escape(&r.path),
                json_escape(&r.title),
                status_str,
                days_overdue_str,
                owner_str,
                updated_str,
                review_every_str,
                comma,
            ));
        }
        pages_json.push_str("  ]");

        println!(
            "{{\n  \"date\":\"{}\",\n  \"summary\":{{\"total\":{},\"fresh\":{},\"due_soon\":{},\"overdue\":{},\"expired\":{},\"never\":{},\"no_freshness\":{}}},\n  \"pages\":{}\n}}",
            today, total, fresh_count, due_soon_count, overdue_count, expired_count, never_count, no_freshness_count, pages_json
        );
    }

    Ok(())
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Parse an ISO `YYYY-MM-DD` date into days since 1970-01-01. Returns
/// `None` on malformed input — the renderer degrades to "not stale."
pub fn parse_iso_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let mut parts = s.split('-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_since_epoch(y, m, d))
}

/// Gregorian (y, m, d) → days since 1970-01-01. JDN formula.
fn days_since_epoch(y: i32, m: u32, d: u32) -> i64 {
    let a = (14 - m as i32) / 12;
    let y = y + 4800 - a;
    let m_adj = m as i32 + 12 * a - 3;
    let jdn = d as i32 + (153 * m_adj + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    (jdn - 2440588) as i64
}

/// Days since 1970-01-01 → ISO `YYYY-MM-DD`. Inverse of `days_since_epoch`.
fn iso_from_days_since_epoch(days: i64) -> String {
    // Offset to JDN, then invert the Gregorian algorithm.
    let jdn = days + 2440588;
    let a = jdn + 32044;
    let b = (4 * a + 3) / 146_097;
    let c = a - (146_097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m_ = (5 * e + 2) / 153;
    let day = e - (153 * m_ + 2) / 5 + 1;
    let month = m_ + 3 - 12 * (m_ / 10);
    let year = 100 * b + d - 4800 + m_ / 10;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Parse a duration string into days. Returns `None` on anything we don't
/// recognize — renderer falls back to "not stale."
pub fn parse_duration_days(s: &str) -> Option<i64> {
    let s = s.trim();
    match s.to_ascii_lowercase().as_str() {
        "weekly" => return Some(7),
        "monthly" => return Some(30),
        "quarterly" => return Some(90),
        "yearly" | "annually" => return Some(365),
        _ => {}
    }
    // Numeric + unit suffix: 7d, 12w, 3m, 1y.
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = num.trim().parse().ok()?;
    let mult = match unit {
        "d" | "D" => 1,
        "w" | "W" => 7,
        "m" | "M" => 30,
        "y" | "Y" => 365,
        _ => return None,
    };
    Some(n * mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso_and_back_round_trips() {
        let cases = [
            "1970-01-01",
            "2026-04-21",
            "2000-02-29",
            "2100-01-01",
            "1999-12-31",
        ];
        for c in cases {
            let d = parse_iso_date(c).expect("parse");
            let back = iso_from_days_since_epoch(d);
            assert_eq!(back, c, "round-trip {c}");
        }
    }

    #[test]
    fn parse_iso_rejects_garbage() {
        assert!(parse_iso_date("not a date").is_none());
        assert!(parse_iso_date("2026-13-01").is_none());
        assert!(parse_iso_date("2026-01-32").is_none());
        assert!(parse_iso_date("2026/01/01").is_none());
    }

    #[test]
    fn duration_parses_numeric_and_word_forms() {
        assert_eq!(parse_duration_days("7d"), Some(7));
        assert_eq!(parse_duration_days("12w"), Some(84));
        assert_eq!(parse_duration_days("3m"), Some(90));
        assert_eq!(parse_duration_days("1y"), Some(365));
        assert_eq!(parse_duration_days("weekly"), Some(7));
        assert_eq!(parse_duration_days("Monthly"), Some(30));
        assert_eq!(parse_duration_days("quarterly"), Some(90));
        assert_eq!(parse_duration_days("yearly"), Some(365));
        assert_eq!(parse_duration_days("annually"), Some(365));
        assert_eq!(parse_duration_days("once in a while"), None);
    }

    #[test]
    fn is_stale_triggers_when_cadence_exceeded() {
        // Page updated 2026-01-01, reviewed every 90 days. On 2026-04-21
        // (110 days later) it should be stale.
        let f = Freshness {
            updated: Some("2026-01-01".to_string()),
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: None,
        };
        let info = info_for(Some(&f), "2026-04-21").unwrap();
        assert_eq!(info.days_since_update(), Some(110));
        assert!(info.is_stale());
    }

    #[test]
    fn is_not_stale_when_within_window() {
        let f = Freshness {
            updated: Some("2026-04-01".to_string()),
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: None,
        };
        let info = info_for(Some(&f), "2026-04-21").unwrap();
        assert!(!info.is_stale());
    }

    #[test]
    fn is_not_stale_when_metadata_incomplete() {
        // Missing review_every → no cadence → never stale.
        let f = Freshness {
            updated: Some("2020-01-01".to_string()),
            review_every: None,
            owner: None,
            sources_of_truth: None,
            expires: None,
        };
        let info = info_for(Some(&f), "2026-04-21").unwrap();
        assert!(!info.is_stale());

        // Missing updated → nothing to compare against.
        let f = Freshness {
            updated: None,
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: None,
        };
        let info = info_for(Some(&f), "2026-04-21").unwrap();
        assert!(!info.is_stale());
    }

    #[test]
    fn expired_when_past_expiry_date() {
        let f = Freshness {
            updated: Some("2026-01-01".to_string()),
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: Some("2026-03-01".to_string()),
        };
        let info = info_for(Some(&f), "2026-04-01").unwrap();
        assert!(matches!(info.status(), FreshnessStatus::Expired { .. }));
    }

    #[test]
    fn not_expired_before_expiry_date() {
        let f = Freshness {
            updated: Some("2026-01-01".to_string()),
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: Some("2026-12-31".to_string()),
        };
        let info = info_for(Some(&f), "2026-05-01").unwrap();
        assert!(matches!(info.status(), FreshnessStatus::Overdue { .. }));
    }

    #[test]
    fn today_honors_env_var() {
        std::env::set_var("KAZAM_TODAY", "2099-06-15");
        assert_eq!(today_iso(), "2099-06-15");
        std::env::remove_var("KAZAM_TODAY");
    }
}
