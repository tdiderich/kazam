use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::freshness::{self, FreshnessStatus};
use crate::types::{Annotation, AnnotationStatus};

pub fn annotations_dir(site_dir: &Path, page_slug: &str) -> PathBuf {
    site_dir.join(".kazam").join("annotations").join(page_slug)
}

pub fn page_slug_from_path(page_path: &str) -> String {
    let stripped = page_path.trim_end_matches(".yaml").trim_end_matches(".yml");
    stripped.replace('/', "--")
}

pub fn generate_id(today: &str) -> String {
    let date_compact = today.replace('-', "");
    let suffix = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        format!("{:04x}", (nanos ^ pid) & 0xFFFF)
    };
    format!("ann-{}-{}", date_compact, suffix)
}

pub fn load_annotations(site_dir: &Path, page_slug: &str) -> Result<Vec<Annotation>> {
    let dir = annotations_dir(site_dir, page_slug);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut annotations = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading annotation dir {:?}", dir))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let content = std::fs::read_to_string(entry.path())
            .with_context(|| format!("reading {:?}", entry.path()))?;
        let ann: Annotation = serde_yaml::from_str(&content)
            .with_context(|| format!("parsing {:?}", entry.path()))?;
        annotations.push(ann);
    }
    Ok(annotations)
}

pub fn save_annotation(site_dir: &Path, page_slug: &str, ann: &Annotation) -> Result<PathBuf> {
    let dir = annotations_dir(site_dir, page_slug);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating annotation dir {:?}", dir))?;
    let file_path = dir.join(format!("{}.yaml", ann.id));
    let yaml = serde_yaml::to_string(ann).context("serializing annotation")?;
    std::fs::write(&file_path, yaml).with_context(|| format!("writing {:?}", file_path))?;
    Ok(file_path)
}

const DEFAULT_ANNOTATION_DECAY_DAYS: i64 = 14;
const ANNOTATION_DUE_SOON_DAYS: i64 = 7;

pub fn annotation_freshness(ann: &Annotation, today: &str) -> FreshnessStatus {
    if ann.status == AnnotationStatus::Incorporated || ann.status == AnnotationStatus::Ignored {
        return FreshnessStatus::Fresh;
    }
    let added = match freshness::parse_iso_date(&ann.added) {
        Some(d) => d,
        None => return FreshnessStatus::Fresh,
    };
    let today_days = match freshness::parse_iso_date(today) {
        Some(d) => d,
        None => return FreshnessStatus::Fresh,
    };
    let elapsed = today_days - added;
    let days_until_due = DEFAULT_ANNOTATION_DECAY_DAYS - elapsed;
    if days_until_due < 0 {
        FreshnessStatus::Overdue {
            days_overdue: -days_until_due,
        }
    } else if days_until_due <= ANNOTATION_DUE_SOON_DAYS {
        FreshnessStatus::DueSoon { days_until_due }
    } else {
        FreshnessStatus::Fresh
    }
}

pub fn clear_annotations(site_dir: &Path, page_slug: &str) -> Result<usize> {
    let dir = annotations_dir(site_dir, page_slug);
    if !dir.exists() {
        return Ok(0);
    }
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading annotation dir {:?}", dir))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false)
        })
        .collect();
    let count = entries.len();
    for entry in entries {
        std::fs::remove_file(entry.path())
            .with_context(|| format!("removing {:?}", entry.path()))?;
    }
    Ok(count)
}

pub fn resolve_annotation(site_dir: &Path, id: &str) -> Result<()> {
    let ann_dir = site_dir.join(".kazam").join("annotations");
    if !ann_dir.exists() {
        anyhow::bail!("annotation not found: {}", id);
    }
    let filename = format!("{}.yaml", id);
    for entry in std::fs::read_dir(&ann_dir).context("reading annotations dir")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let candidate = entry.path().join(&filename);
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate)
                .with_context(|| format!("reading {:?}", candidate))?;
            let mut ann: Annotation = serde_yaml::from_str(&content)
                .with_context(|| format!("parsing {:?}", candidate))?;
            ann.status = AnnotationStatus::Incorporated;
            let yaml = serde_yaml::to_string(&ann).context("serializing annotation")?;
            std::fs::write(&candidate, yaml).with_context(|| format!("writing {:?}", candidate))?;
            return Ok(());
        }
    }
    anyhow::bail!("annotation not found: {}", id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AnnotationSource, AnnotationStatus};

    #[test]
    fn page_slug_strips_extension_and_separates() {
        assert_eq!(page_slug_from_path("deals/acme.yaml"), "deals--acme");
        assert_eq!(page_slug_from_path("index.yaml"), "index");
        assert_eq!(
            page_slug_from_path("customers/enterprise/big-co.yaml"),
            "customers--enterprise--big-co"
        );
    }

    #[test]
    fn generate_id_format() {
        let id = generate_id("2026-05-07");
        assert!(id.starts_with("ann-20260507-"));
        assert_eq!(id.len(), "ann-20260507-".len() + 4);
    }

    #[test]
    fn annotation_round_trip_all_fields() {
        let ann = Annotation {
            id: "ann-20260507-a3f2".into(),
            text: "Customer evaluating Wiz".into(),
            author: "tyler".into(),
            section: Some("competitive".into()),
            added: "2026-05-07".into(),
            status: AnnotationStatus::Pending,
            source: AnnotationSource::Cli,
        };
        let yaml = serde_yaml::to_string(&ann).unwrap();
        let parsed: Annotation = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.id, ann.id);
        assert_eq!(parsed.text, ann.text);
        assert_eq!(parsed.author, ann.author);
        assert_eq!(parsed.section, ann.section);
        assert_eq!(parsed.added, ann.added);
        assert_eq!(parsed.status, AnnotationStatus::Pending);
        assert_eq!(parsed.source, AnnotationSource::Cli);
    }

    #[test]
    fn annotation_round_trip_minimal() {
        let yaml = r#"
id: ann-20260507-0001
text: "quick note"
author: tyler
added: "2026-05-07"
"#;
        let ann: Annotation = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ann.id, "ann-20260507-0001");
        assert_eq!(ann.section, None);
        assert_eq!(ann.status, AnnotationStatus::Pending);
        assert_eq!(ann.source, AnnotationSource::Cli);
    }

    #[test]
    fn annotation_rejects_invalid_status() {
        let yaml = r#"
id: ann-20260507-0001
text: note
author: tyler
added: "2026-05-07"
status: bogus
"#;
        let result = serde_yaml::from_str::<Annotation>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn annotation_rejects_invalid_source() {
        let yaml = r#"
id: ann-20260507-0001
text: note
author: tyler
added: "2026-05-07"
source: slack
"#;
        let result = serde_yaml::from_str::<Annotation>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn save_and_load_annotations() {
        let tmp = std::env::temp_dir().join(format!("kazam-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let ann = Annotation {
            id: "ann-20260507-beef".into(),
            text: "test annotation".into(),
            author: "tyler".into(),
            section: None,
            added: "2026-05-07".into(),
            status: AnnotationStatus::Pending,
            source: AnnotationSource::Agent,
        };

        save_annotation(&tmp, "deals--acme", &ann).unwrap();
        let loaded = load_annotations(&tmp, "deals--acme").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "ann-20260507-beef");
        assert_eq!(loaded[0].source, AnnotationSource::Agent);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_annotations_empty_dir() {
        let tmp = std::env::temp_dir().join(format!("kazam-test-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let result = load_annotations(&tmp, "nonexistent");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn make_annotation(added: &str, status: AnnotationStatus) -> Annotation {
        Annotation {
            id: "ann-test-0001".into(),
            text: "test".into(),
            author: "tyler".into(),
            section: None,
            added: added.into(),
            status,
            source: AnnotationSource::Cli,
        }
    }

    #[test]
    fn annotation_fresh_within_window() {
        let ann = make_annotation("2026-05-01", AnnotationStatus::Pending);
        let status = annotation_freshness(&ann, "2026-05-05");
        assert!(matches!(status, FreshnessStatus::Fresh));
    }

    #[test]
    fn annotation_due_soon_near_decay() {
        let ann = make_annotation("2026-04-25", AnnotationStatus::Pending);
        // 12 days elapsed, 2 days until 14-day decay = within DUE_SOON_WINDOW_DAYS(7)
        let status = annotation_freshness(&ann, "2026-05-07");
        assert!(matches!(status, FreshnessStatus::DueSoon { .. }));
    }

    #[test]
    fn annotation_stale_past_decay() {
        let ann = make_annotation("2026-04-20", AnnotationStatus::Pending);
        // 17 days elapsed > 14-day decay
        let status = annotation_freshness(&ann, "2026-05-07");
        assert!(matches!(status, FreshnessStatus::Overdue { days_overdue } if days_overdue == 3));
    }

    #[test]
    fn incorporated_skips_decay() {
        let ann = make_annotation("2026-01-01", AnnotationStatus::Incorporated);
        let status = annotation_freshness(&ann, "2026-05-07");
        assert!(matches!(status, FreshnessStatus::Fresh));
    }

    #[test]
    fn ignored_skips_decay() {
        let ann = make_annotation("2026-01-01", AnnotationStatus::Ignored);
        let status = annotation_freshness(&ann, "2026-05-07");
        assert!(matches!(status, FreshnessStatus::Fresh));
    }

    #[test]
    fn clear_annotations_removes_all() {
        let tmp = std::env::temp_dir().join(format!("kazam-test-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        let ann1 = Annotation {
            id: "ann-20260507-0001".into(),
            text: "first".into(),
            author: "tyler".into(),
            section: None,
            added: "2026-05-07".into(),
            status: AnnotationStatus::Pending,
            source: AnnotationSource::Cli,
        };
        let ann2 = Annotation {
            id: "ann-20260507-0002".into(),
            text: "second".into(),
            author: "tyler".into(),
            section: None,
            added: "2026-05-07".into(),
            status: AnnotationStatus::Pending,
            source: AnnotationSource::Cli,
        };
        save_annotation(&tmp, "test--page", &ann1).unwrap();
        save_annotation(&tmp, "test--page", &ann2).unwrap();
        assert_eq!(load_annotations(&tmp, "test--page").unwrap().len(), 2);

        let count = clear_annotations(&tmp, "test--page").unwrap();
        assert_eq!(count, 2);
        assert!(load_annotations(&tmp, "test--page").unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn clear_annotations_empty_returns_zero() {
        let tmp =
            std::env::temp_dir().join(format!("kazam-test-clear-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let count = clear_annotations(&tmp, "nonexistent").unwrap();
        assert_eq!(count, 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_annotation_sets_incorporated() {
        let tmp = std::env::temp_dir().join(format!("kazam-test-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        let ann = Annotation {
            id: "ann-20260507-cafe".into(),
            text: "resolve me".into(),
            author: "tyler".into(),
            section: None,
            added: "2026-05-07".into(),
            status: AnnotationStatus::Pending,
            source: AnnotationSource::Cli,
        };
        save_annotation(&tmp, "test--page", &ann).unwrap();

        resolve_annotation(&tmp, "ann-20260507-cafe").unwrap();

        let loaded = load_annotations(&tmp, "test--page").unwrap();
        assert_eq!(loaded[0].status, AnnotationStatus::Incorporated);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_annotation_not_found() {
        let tmp =
            std::env::temp_dir().join(format!("kazam-test-resolve-nf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".kazam/annotations/empty")).unwrap();

        let result = resolve_annotation(&tmp, "ann-20260507-0000");
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
