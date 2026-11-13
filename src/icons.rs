//! Bundled lucide icons. Each icon is stored as the inner SVG content
//! (paths/circles/etc.) without the outer `<svg>` wrapper so the renderer
//! can apply size/color at render time.
//!
//! Source: https://lucide.dev (ISC license).

pub fn get(name: &str) -> Option<&'static str> {
    match name {
        // Navigation
        "arrow-left" => Some(r#"<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>"#),
        "arrow-right" => Some(r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#),
        "arrow-up-right" => Some(r#"<path d="M7 7h10v10"/><path d="M7 17 17 7"/>"#),
        "chevron-left" => Some(r#"<path d="m15 18-6-6 6-6"/>"#),
        "chevron-right" => Some(r#"<path d="m9 18 6-6-6-6"/>"#),
        "chevron-down" => Some(r#"<path d="m6 9 6 6 6-6"/>"#),
        // Actions
        "check" => Some(r#"<path d="M20 6 9 17l-5-5"/>"#),
        "x" => Some(r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#),
        "plus" => Some(r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#),
        "search" => Some(r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#),
        "play" => Some(r#"<polygon points="6 3 20 12 6 21 6 3"/>"#),
        "download" => Some(
            r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/>"#,
        ),
        // Status
        "info" => {
            Some(r#"<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#)
        }
        "alert-triangle" => Some(
            r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#,
        ),
        "alert-circle" => {
            Some(r#"<circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/>"#)
        }
        "check-circle" => Some(r#"<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>"#),
        "x-circle" => {
            Some(r#"<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>"#)
        }
        // Objects
        "file" => Some(
            r#"<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/><polyline points="14 2 14 8 20 8"/>"#,
        ),
        "folder" => Some(
            r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z"/>"#,
        ),
        "link" => Some(
            r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
        ),
        "mail" => Some(
            r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>"#,
        ),
        "inbox" => Some(
            r#"<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>"#,
        ),
        "lock" => Some(
            r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#,
        ),
        "bell" => Some(
            r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>"#,
        ),
        "calendar" => Some(
            r#"<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/>"#,
        ),
        "clock" => Some(r#"<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>"#),
        // People
        "user" => Some(
            r#"<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#,
        ),
        "users" => Some(
            r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#,
        ),
        // UI
        "home" => Some(
            r#"<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/>"#,
        ),
        "menu" => Some(
            r#"<line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="18" y2="18"/>"#,
        ),
        "settings" => Some(
            r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#,
        ),
        // Brands (simple shapes only)
        "github" => Some(
            r#"<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>"#,
        ),
        _ => None,
    }
}

/// Render an icon as a complete inline SVG string sized to `size` pixels.
/// `color` is a CSS color value (e.g. `"currentColor"`, `"#3CCECE"`).
pub fn render(name: &str, size: u32, color: &str) -> String {
    match get(name) {
        Some(inner) => format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{inner}</svg>"#
        ),
        None => String::new(),
    }
}
