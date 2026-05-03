//! `kazam voice` — prints the site's voice configuration from `kazam.yaml`.

use anyhow::Result;
use std::path::Path;

use crate::build::load_config;
use crate::types::Voice;

pub fn run(dir: &Path, json: bool) -> Result<()> {
    let config = load_config(dir)?;

    match &config.voice {
        None => {
            eprintln!(
                "No voice configuration found for \"{}\".",
                config.name
            );
            eprintln!("Add a `voice:` section to kazam.yaml to define tone, reading level, and terminology.");
            eprintln!("\nExample:");
            eprintln!("  voice:");
            eprintln!("    tone: \"direct, technical, no marketing fluff\"");
            eprintln!("    reading_level: \"senior engineer\"");
            eprintln!("    terminology:");
            eprintln!("      prefer:");
            eprintln!("        KB: \"knowledge base\"");
            eprintln!("      avoid:");
            eprintln!("        - synergy");
        }
        Some(voice) => {
            if json {
                print_json(voice)?;
            } else {
                print_human(&config.name, voice);
            }
        }
    }

    Ok(())
}

fn print_human(site_name: &str, voice: &Voice) {
    println!("Voice configuration for \"{}\":", site_name);
    if let Some(tone) = &voice.tone {
        println!("  Tone: {}", tone);
    }
    if let Some(level) = &voice.reading_level {
        println!("  Reading level: {}", level);
    }
    if let Some(term) = &voice.terminology {
        if !term.prefer.is_empty() {
            let mut prefer: Vec<(&String, &String)> = term.prefer.iter().collect();
            prefer.sort_by_key(|(k, _)| *k);
            for (avoid, use_instead) in prefer {
                println!("  Prefer: \"{}\" over \"{}\"", use_instead, avoid);
            }
        }
        if !term.avoid.is_empty() {
            println!("  Avoid: {}", term.avoid.join(", "));
        }
    }
}

fn print_json(voice: &Voice) -> Result<()> {
    // Build a JSON object manually so we only include populated fields and
    // keep the output clean rather than emitting nulls everywhere.
    use serde_json::{json, Map, Value};

    let mut obj = Map::new();
    if let Some(tone) = &voice.tone {
        obj.insert("tone".into(), Value::String(tone.clone()));
    }
    if let Some(level) = &voice.reading_level {
        obj.insert("reading_level".into(), Value::String(level.clone()));
    }
    if let Some(term) = &voice.terminology {
        let mut term_obj = Map::new();
        if !term.prefer.is_empty() {
            let prefer_obj: serde_json::Map<String, Value> = term
                .prefer
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            term_obj.insert("prefer".into(), Value::Object(prefer_obj));
        }
        if !term.avoid.is_empty() {
            term_obj.insert(
                "avoid".into(),
                json!(term.avoid),
            );
        }
        if !term_obj.is_empty() {
            obj.insert("terminology".into(), Value::Object(term_obj));
        }
    }

    println!("{}", serde_json::to_string_pretty(&Value::Object(obj))?);
    Ok(())
}
