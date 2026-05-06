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

const DRAFT_STALE_DAYS: i64 = 30;

/// True when the page's freshness metadata has an `expires` date that is
/// in the past relative to `today_iso`. Used by `Page::is_archived()`.
pub fn is_expired(f: Option<&Freshness>, today_iso: &str) -> bool {
    let f = match f {
        Some(f) => f,
        None => return false,
    };
    let expires = match f.expires.as_deref().and_then(parse_iso_date) {
        Some(d) => d,
        None => return false,
    };
    let today = parse_iso_date(today_iso).unwrap_or(0);
    today > expires
}

/// True when a draft page has gone 30+ days without an update. Drafts
/// without an `updated` date are never auto-archived (they're timeless
/// until someone sets a date).
pub fn is_stale_draft(f: Option<&Freshness>, today_iso: &str) -> bool {
    let f = match f {
        Some(f) => f,
        None => return false,
    };
    let updated = match f.updated.as_deref().and_then(parse_iso_date) {
        Some(d) => d,
        None => return false,
    };
    let today = parse_iso_date(today_iso).unwrap_or(0);
    today - updated >= DRAFT_STALE_DAYS
}

// ── Refresh serialization helper ──────────────────────────────────────────

/// Serialize an `Option<RefreshValue>` to a JSON string fragment.
/// Used by both the `freshness show` and `freshness review` JSON outputs.
fn serialize_refresh_json(refresh: &Option<crate::types::RefreshValue>) -> String {
    use crate::types::{RefreshMode, RefreshStep, RefreshValue};
    match refresh {
        None => "null".to_string(),
        Some(RefreshValue::Prompt(s)) => format!("\"{}\"", json_escape(s)),
        Some(RefreshValue::Full(config)) => {
            let mode = match config.mode {
                RefreshMode::Human => "human",
                RefreshMode::Auto => "auto",
                RefreshMode::Assisted => "assisted",
            };
            let steps: Vec<String> = config
                .steps
                .iter()
                .map(|step| match step {
                    RefreshStep::Run(v) => format!("{{\"run\":\"{}\"}}", json_escape(v)),
                    RefreshStep::Prompt(v) => format!("{{\"prompt\":\"{}\"}}", json_escape(v)),
                    RefreshStep::Review(v) => format!("{{\"review\":\"{}\"}}", json_escape(v)),
                })
                .collect();
            format!("{{\"mode\":\"{}\",\"steps\":[{}]}}", mode, steps.join(","))
        }
    }
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
        refresh: Option<crate::types::RefreshValue>,
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
                    refresh: None,
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
                    refresh: None,
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
                    refresh: f.refresh.clone(),
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
                    refresh: f.refresh.clone(),
                });
            }
        }
    }

    // Compute summary counts
    let total = results.len();
    let fresh_count = results
        .iter()
        .filter(|r| matches!(r.status, FreshnessStatus::Fresh) && !r.is_never && !r.no_freshness)
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
                println!(
                    "  [{status}] {path}{owner}",
                    status = status_str,
                    path = r.path
                );
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
            let refresh_str = serialize_refresh_json(&r.refresh);

            let comma = if i + 1 < non_fresh.len() { "," } else { "" };
            pages_json.push_str(&format!(
                "    {{\"path\":\"{}\",\"title\":\"{}\",\"status\":\"{}\",\"days_overdue\":{},\"owner\":{},\"updated\":{},\"review_every\":{},\"refresh\":{}}}{}\n",
                json_escape(&r.path),
                json_escape(&r.title),
                status_str,
                days_overdue_str,
                owner_str,
                updated_str,
                review_every_str,
                refresh_str,
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

/// `kazam freshness review` — list stale pages with recommended actions.
/// Each page gets a recommendation: "archive" for expired or 180+ days overdue,
/// "refresh" for moderately overdue, "review" for recently due.
pub fn run_review(dir: &Path, json: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::fs;
    use walkdir::WalkDir;

    let today = today_iso();

    #[allow(dead_code)]
    struct ReviewItem {
        path: String,
        title: String,
        status: FreshnessStatus,
        days: i64,
        owner: Option<String>,
        updated: Option<String>,
        cadence: Option<String>,
        recommendation: &'static str,
        description: Option<String>,
        refresh: Option<crate::types::RefreshValue>,
    }

    let mut items: Vec<ReviewItem> = Vec::new();

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
        if !path.extension().map(|e| e == "yaml").unwrap_or(false) {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let content = fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let page: crate::types::Page =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

        let fv = page.freshness.as_ref();
        if fv.is_none() || fv.map(|v| v.is_never()).unwrap_or(false) {
            continue;
        }
        let f = fv.and_then(|v| v.as_full());
        let info = match info_for(f, &today) {
            Some(i) => i,
            None => continue,
        };
        let status = info.status();
        let (days, recommendation) = match status {
            FreshnessStatus::Expired { days_past_expiry } => (days_past_expiry, "archive"),
            FreshnessStatus::Overdue { days_overdue } if days_overdue > 180 => {
                (days_overdue, "archive")
            }
            FreshnessStatus::Overdue { days_overdue } => (days_overdue, "refresh"),
            FreshnessStatus::DueSoon { days_until_due } => (days_until_due, "review"),
            FreshnessStatus::Fresh => continue,
        };

        items.push(ReviewItem {
            path: rel_str,
            title: page.title,
            status,
            days,
            owner: f.and_then(|f| f.owner.clone()),
            updated: f.and_then(|f| f.updated.clone()),
            cadence: f.and_then(|f| f.review_every.clone()),
            recommendation,
            description: page.subtitle,
            refresh: f.and_then(|f| f.refresh.clone()),
        });
    }

    items.sort_by(|a, b| {
        fn rank(s: &FreshnessStatus) -> u8 {
            match s {
                FreshnessStatus::Expired { .. } => 0,
                FreshnessStatus::Overdue { .. } => 1,
                FreshnessStatus::DueSoon { .. } => 2,
                FreshnessStatus::Fresh => 3,
            }
        }
        rank(&a.status)
            .cmp(&rank(&b.status))
            .then(b.days.cmp(&a.days))
    });

    if json {
        println!(
            "{{\"date\":\"{}\",\"count\":{},\"items\":[",
            today,
            items.len()
        );
        for (i, item) in items.iter().enumerate() {
            let status_str = match item.status {
                FreshnessStatus::Expired { .. } => "expired",
                FreshnessStatus::Overdue { .. } => "overdue",
                FreshnessStatus::DueSoon { .. } => "due_soon",
                FreshnessStatus::Fresh => "fresh",
            };
            let owner = item
                .owner
                .as_deref()
                .map(|o| format!("\"{}\"", json_escape(o)))
                .unwrap_or("null".into());
            let refresh_str = serialize_refresh_json(&item.refresh);
            let comma = if i + 1 < items.len() { "," } else { "" };
            println!(
                "  {{\"path\":\"{}\",\"title\":\"{}\",\"status\":\"{}\",\"days\":{},\"owner\":{},\"recommendation\":\"{}\",\"refresh\":{}}}{}",
                json_escape(&item.path),
                json_escape(&item.title),
                status_str,
                item.days,
                owner,
                item.recommendation,
                refresh_str,
                comma,
            );
        }
        println!("]}}");
    } else {
        println!("Freshness review — {}", today);
        println!("{} page(s) need attention\n", items.len());
        for item in &items {
            let status_label = match item.status {
                FreshnessStatus::Expired { days_past_expiry } => {
                    format!("EXPIRED {} day(s)", days_past_expiry)
                }
                FreshnessStatus::Overdue { days_overdue } => {
                    format!("OVERDUE {} day(s)", days_overdue)
                }
                FreshnessStatus::DueSoon { days_until_due } => {
                    format!("DUE in {} day(s)", days_until_due)
                }
                FreshnessStatus::Fresh => "FRESH".into(),
            };
            let owner = item
                .owner
                .as_deref()
                .map(|o| format!(" — owner: {}", o))
                .unwrap_or_default();
            let updated = item
                .updated
                .as_deref()
                .map(|u| format!(" (updated: {})", u))
                .unwrap_or_default();
            println!(
                "  [{}] {} → {}",
                status_label, item.path, item.recommendation
            );
            println!("    {}{}{}\n", item.title, owner, updated);
        }
        println!("Actions:");
        println!("  kazam freshness act <path> archive   # set archived: true");
        println!("  kazam freshness act <path> refresh   # update freshness.updated to today");
    }

    Ok(())
}

/// `kazam freshness act` — take action on a stale page.
pub fn run_act(dir: &Path, rel_path: &str, action: &crate::FreshnessAction) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::fs;

    let path = dir.join(rel_path);
    if !path.exists() {
        anyhow::bail!("page not found: {}", path.display());
    }

    let content = fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))?;

    match action {
        crate::FreshnessAction::Archive => {
            let new_content = if content.contains("\narchived:") {
                content.replace("\narchived: false", "\narchived: true")
            } else {
                let insert_pos = content.find("\ncomponents:").unwrap_or(content.len());
                let mut out = String::with_capacity(content.len() + 20);
                out.push_str(&content[..insert_pos]);
                out.push_str("\narchived: true");
                out.push_str(&content[insert_pos..]);
                out
            };
            fs::write(&path, new_content)?;
            println!("✓ archived {}", rel_path);
        }
        crate::FreshnessAction::Refresh => {
            let today = today_iso();
            let new_content = if let Some(start) = content.find("  updated:") {
                let line_end = content[start..]
                    .find('\n')
                    .map(|p| start + p)
                    .unwrap_or(content.len());
                format!(
                    "{}  updated: \"{}\"{}",
                    &content[..start],
                    today,
                    &content[line_end..]
                )
            } else if let Some(start) = content.find("freshness:") {
                let insert = start + "freshness:".len();
                let after = &content[insert..];
                let next_line_end = after
                    .find('\n')
                    .map(|p| insert + p + 1)
                    .unwrap_or(content.len());
                format!(
                    "{}  updated: \"{}\"\n{}",
                    &content[..next_line_end],
                    today,
                    &content[next_line_end..]
                )
            } else {
                anyhow::bail!("no freshness metadata in {}", rel_path);
            };
            fs::write(&path, new_content)?;
            println!("✓ refreshed {} → updated: {}", rel_path, today);
        }
    }

    Ok(())
}

/// `kazam freshness notify` — stale pages grouped by owner, formatted for
/// Slack or email. Outputs markdown by default, JSON with `--json`.
pub fn run_notify(dir: &Path, json: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::collections::BTreeMap;
    use std::fs;
    use walkdir::WalkDir;

    let today = today_iso();

    struct NotifyItem {
        path: String,
        title: String,
        status: FreshnessStatus,
        days: i64,
    }

    let mut by_owner: BTreeMap<String, Vec<NotifyItem>> = BTreeMap::new();

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
        if !path.extension().map(|e| e == "yaml").unwrap_or(false) {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let content = fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let page: crate::types::Page =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

        let fv = page.freshness.as_ref();
        if fv.is_none() || fv.map(|v| v.is_never()).unwrap_or(false) {
            continue;
        }
        let f = fv.and_then(|v| v.as_full());
        let info = match info_for(f, &today) {
            Some(i) => i,
            None => continue,
        };
        let status = info.status();
        let days = match status {
            FreshnessStatus::Expired { days_past_expiry } => days_past_expiry,
            FreshnessStatus::Overdue { days_overdue } => days_overdue,
            FreshnessStatus::DueSoon { days_until_due } => days_until_due,
            FreshnessStatus::Fresh => continue,
        };

        let owner = f
            .and_then(|f| f.owner.clone())
            .unwrap_or_else(|| "(unowned)".to_string());

        by_owner.entry(owner).or_default().push(NotifyItem {
            path: rel_str,
            title: page.title,
            status,
            days,
        });
    }

    // Sort items within each owner group: expired first, then overdue (most first), then due_soon
    for items in by_owner.values_mut() {
        items.sort_by(|a, b| {
            fn rank(s: &FreshnessStatus) -> u8 {
                match s {
                    FreshnessStatus::Expired { .. } => 0,
                    FreshnessStatus::Overdue { .. } => 1,
                    FreshnessStatus::DueSoon { .. } => 2,
                    FreshnessStatus::Fresh => 3,
                }
            }
            rank(&a.status)
                .cmp(&rank(&b.status))
                .then(b.days.cmp(&a.days))
        });
    }

    let total_items: usize = by_owner.values().map(|v| v.len()).sum();

    if json {
        let mut owners_json = Vec::new();
        for (owner, items) in &by_owner {
            let mut items_json = Vec::new();
            for item in items {
                let status_str = match item.status {
                    FreshnessStatus::Expired { .. } => "expired",
                    FreshnessStatus::Overdue { .. } => "overdue",
                    FreshnessStatus::DueSoon { .. } => "due_soon",
                    FreshnessStatus::Fresh => "fresh",
                };
                items_json.push(format!(
                    "{{\"path\":\"{}\",\"title\":\"{}\",\"status\":\"{}\",\"days\":{}}}",
                    json_escape(&item.path),
                    json_escape(&item.title),
                    status_str,
                    item.days,
                ));
            }
            owners_json.push(format!(
                "{{\"owner\":\"{}\",\"count\":{},\"pages\":[{}]}}",
                json_escape(owner),
                items.len(),
                items_json.join(","),
            ));
        }
        println!(
            "{{\"date\":\"{}\",\"total\":{},\"owners\":[{}]}}",
            today,
            total_items,
            owners_json.join(","),
        );
    } else {
        if by_owner.is_empty() {
            println!("All pages are fresh — nothing to notify.");
            return Ok(());
        }
        println!("**Freshness digest — {}**\n", today);
        println!(
            "{} page(s) need attention across {} owner(s)\n",
            total_items,
            by_owner.len()
        );
        for (owner, items) in &by_owner {
            println!(
                "**{}** ({} page{})",
                owner,
                items.len(),
                if items.len() == 1 { "" } else { "s" }
            );
            for item in items {
                let badge = match item.status {
                    FreshnessStatus::Expired { days_past_expiry } => {
                        format!("EXPIRED {} days", days_past_expiry)
                    }
                    FreshnessStatus::Overdue { days_overdue } => {
                        format!("OVERDUE {} days", days_overdue)
                    }
                    FreshnessStatus::DueSoon { days_until_due } => {
                        format!("due in {} days", days_until_due)
                    }
                    FreshnessStatus::Fresh => "fresh".into(),
                };
                println!("  - [{}] {} ({})", badge, item.title, item.path);
            }
            println!();
        }
    }

    Ok(())
}

/// `kazam freshness drift` — check git history for source-of-truth files to
/// detect when upstream code has changed since a documentation page was last
/// reviewed.
pub fn run_drift(dir: &Path, pretty: bool, cli_repos: Vec<String>) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::fs;
    use std::process::Command;
    use walkdir::WalkDir;

    let today = today_iso();

    // Load config drift repos
    let config_path = dir.join("kazam.yaml");
    let mut repos: Vec<(String, String)> = Vec::new(); // (prefix, local)
    if config_path.exists() {
        let config_content = fs::read_to_string(&config_path)
            .with_context(|| format!("reading {:?}", config_path))?;
        let site: crate::types::SiteConfig = serde_yaml::from_str(&config_content)
            .with_context(|| format!("parsing {:?}", config_path))?;
        if let Some(drift) = site.drift {
            for repo in drift.repos {
                repos.push((repo.prefix, repo.local));
            }
        }
    }

    // Parse CLI --repo flags (PREFIX=LOCAL) and prepend (CLI takes precedence)
    let mut cli_repo_pairs: Vec<(String, String)> = Vec::new();
    for r in &cli_repos {
        if let Some(eq) = r.find('=') {
            let prefix = r[..eq].to_string();
            let local = r[eq + 1..].to_string();
            cli_repo_pairs.push((prefix, local));
        }
    }
    // CLI repos go first so they match before config repos
    let all_repos: Vec<(String, String)> = cli_repo_pairs.into_iter().chain(repos).collect();

    // Expand ~ in local paths
    let home = std::env::var("HOME").unwrap_or_default();
    let expand_tilde = |p: &str| -> String {
        if let Some(stripped) = p.strip_prefix("~/") {
            format!("{}/{}", home, stripped)
        } else if p == "~" {
            home.clone()
        } else {
            p.to_string()
        }
    };

    struct DriftSource {
        label: String,
        href: String,
        commits: usize,
        latest: String,
    }

    struct DriftPage {
        path: String,
        title: String,
        updated: String,
        owner: Option<String>,
        sources: Vec<DriftSource>,
    }

    let mut drifted: Vec<DriftPage> = Vec::new();
    let mut unmapped: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut clean = 0usize;

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
        if !path.extension().map(|e| e == "yaml").unwrap_or(false) {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let content = fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let page: crate::types::Page =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

        // Skip pages with no freshness, freshness: never, or no sources_of_truth
        let fv = page.freshness.as_ref();
        if fv.is_none() || fv.map(|v| v.is_never()).unwrap_or(false) {
            continue;
        }
        let f = match fv.and_then(|v| v.as_full()) {
            Some(f) => f,
            None => continue,
        };
        let updated = match f.updated.as_deref() {
            Some(u) => u,
            None => continue,
        };
        let sources = match f.sources_of_truth.as_ref() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        total += 1;

        let owner = f.owner.clone();
        let title = page.title.clone();

        let mut page_drifted_sources: Vec<DriftSource> = Vec::new();

        for sot in sources {
            let href = sot.href();
            let label = sot.label();

            // Find matching repo prefix
            let matched = all_repos
                .iter()
                .find(|(prefix, _)| href.starts_with(prefix.as_str()));

            match matched {
                None => {
                    // Unmapped source — not an error, just collect unique ones
                    if !unmapped.contains(&href.to_string()) {
                        unmapped.push(href.to_string());
                    }
                }
                Some((prefix, local)) => {
                    let relative_path = &href[prefix.len()..];
                    // Strip leading slash if present
                    let relative_path = relative_path.trim_start_matches('/');
                    let local_expanded = expand_tilde(local);

                    let output = Command::new("git")
                        .arg("-C")
                        .arg(&local_expanded)
                        .arg("log")
                        .arg(format!("--since={}", updated))
                        .arg("--oneline")
                        .arg("--")
                        .arg(relative_path)
                        .output();

                    match output {
                        Err(_) => {
                            // git not available or repo not found — skip silently
                        }
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let lines: Vec<&str> =
                                stdout.lines().filter(|l| !l.trim().is_empty()).collect();
                            let commit_count = lines.len();
                            if commit_count > 0 {
                                let latest = lines[0].to_string();
                                page_drifted_sources.push(DriftSource {
                                    label: label.to_string(),
                                    href: href.to_string(),
                                    commits: commit_count,
                                    latest,
                                });
                            }
                        }
                    }
                }
            }
        }

        if page_drifted_sources.is_empty() {
            clean += 1;
        } else {
            drifted.push(DriftPage {
                path: rel_str,
                title,
                updated: updated.to_string(),
                owner,
                sources: page_drifted_sources,
            });
        }
    }

    // Sort drifted pages by total commit count descending
    drifted.sort_by(|a, b| {
        let a_total: usize = a.sources.iter().map(|s| s.commits).sum();
        let b_total: usize = b.sources.iter().map(|s| s.commits).sum();
        b_total.cmp(&a_total)
    });

    let unmapped_count = unmapped.len();
    let drifted_count = drifted.len();

    if pretty {
        println!("Freshness drift — {}", today);
        println!(
            "  {} pages, {} drifted, {} clean, {} unmapped sources",
            total, drifted_count, clean, unmapped_count
        );
        if !drifted.is_empty() {
            println!();
            for page in &drifted {
                let owner_str = page
                    .owner
                    .as_deref()
                    .map(|o| format!("  owner: {}", o))
                    .unwrap_or_default();
                println!("  [DRIFTED] {}", page.path);
                println!(
                    "    \"{}\"{}  updated: {}",
                    page.title, owner_str, page.updated
                );
                for src in &page.sources {
                    println!(
                        "    → {}: {} commits (latest: {})",
                        src.label, src.commits, src.latest
                    );
                }
                println!();
            }
        }
        if !unmapped.is_empty() {
            println!("  Unmapped sources (no repo prefix matched):");
            for u in &unmapped {
                println!("    {}", u);
            }
        }
    } else {
        // JSON output
        let mut drifted_json = String::from("[\n");
        for (i, page) in drifted.iter().enumerate() {
            let owner_str = page
                .owner
                .as_deref()
                .map(|o| format!("\"{}\"", json_escape(o)))
                .unwrap_or_else(|| "null".to_string());
            let mut sources_json = String::from("[");
            for (j, src) in page.sources.iter().enumerate() {
                let src_comma = if j + 1 < page.sources.len() { "," } else { "" };
                sources_json.push_str(&format!(
                    "{{\"label\":\"{}\",\"href\":\"{}\",\"commits\":{},\"latest\":\"{}\"}}{}",
                    json_escape(&src.label),
                    json_escape(&src.href),
                    src.commits,
                    json_escape(&src.latest),
                    src_comma,
                ));
            }
            sources_json.push(']');
            let page_comma = if i + 1 < drifted.len() { "," } else { "" };
            drifted_json.push_str(&format!(
                "    {{\"page\":\"{}\",\"title\":\"{}\",\"updated\":\"{}\",\"owner\":{},\"sources\":{}}}{}\n",
                json_escape(&page.path),
                json_escape(&page.title),
                json_escape(&page.updated),
                owner_str,
                sources_json,
                page_comma,
            ));
        }
        drifted_json.push_str("  ]");

        let mut unmapped_json = String::from("[");
        for (i, u) in unmapped.iter().enumerate() {
            let comma = if i + 1 < unmapped.len() { "," } else { "" };
            unmapped_json.push_str(&format!("\"{}\"{}", json_escape(u), comma));
        }
        unmapped_json.push(']');

        println!(
            "{{\n  \"date\":\"{}\",\n  \"summary\":{{\"total\":{},\"drifted\":{},\"clean\":{},\"unmapped\":{}}},\n  \"drifted\":{},\n  \"unmapped\":{}\n}}",
            today, total, drifted_count, clean, unmapped_count, drifted_json, unmapped_json
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
            refresh: None,
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
            refresh: None,
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
            refresh: None,
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
            refresh: None,
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
            refresh: None,
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
            refresh: None,
        };
        let info = info_for(Some(&f), "2026-05-01").unwrap();
        assert!(matches!(info.status(), FreshnessStatus::Overdue { .. }));
    }

    #[test]
    fn stale_draft_after_30_days() {
        let f = Freshness {
            updated: Some("2026-01-01".to_string()),
            review_every: None,
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        };
        assert!(is_stale_draft(Some(&f), "2026-02-01"));
        assert!(!is_stale_draft(Some(&f), "2026-01-20"));
    }

    #[test]
    fn stale_draft_without_updated_is_not_stale() {
        let f = Freshness {
            updated: None,
            review_every: None,
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        };
        assert!(!is_stale_draft(Some(&f), "2026-06-01"));
    }

    #[test]
    fn is_expired_helper() {
        let f = Freshness {
            updated: None,
            review_every: None,
            owner: None,
            sources_of_truth: None,
            expires: Some("2026-03-01".to_string()),
            refresh: None,
        };
        assert!(is_expired(Some(&f), "2026-04-01"));
        assert!(!is_expired(Some(&f), "2026-02-01"));
        assert!(!is_expired(None, "2026-04-01"));
    }

    #[test]
    fn today_honors_env_var() {
        std::env::set_var("KAZAM_TODAY", "2099-06-15");
        assert_eq!(today_iso(), "2099-06-15");
        std::env::remove_var("KAZAM_TODAY");
    }
}
