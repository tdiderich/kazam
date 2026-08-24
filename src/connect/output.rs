//! Output rendering: terminal summaries, page-YAML file output, and a curata
//! write stub (the MCP/API integration is secondary per the task brief -
//! this prints the page YAML that would be sent).

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::connect::aggregate::{self, AggState};
use crate::connect::config::HostConfig;
use crate::connect::types::MappingFile;

pub fn render(
    mapping: &MappingFile,
    shape_results: &HashMap<String, AggState>,
    target: &str,
    dry_run: bool,
    force: bool,
    connector_dir: &Path,
    host: &HostConfig,
) -> Result<()> {
    match target {
        "terminal" => render_terminal(mapping, shape_results),
        "file" => {
            let page = build_page(mapping, shape_results);
            write_file(mapping, connector_dir, &page)
        }
        "both" => {
            render_terminal(mapping, shape_results)?;
            render_curata(mapping, shape_results, dry_run, force, host)
        }
        _ => render_curata(mapping, shape_results, dry_run, force, host),
    }
}

fn render_terminal(mapping: &MappingFile, shape_results: &HashMap<String, AggState>) -> Result<()> {
    println!("\n=== {} ===\n", mapping.mapping);
    for (name, shape) in &mapping.shapes {
        let Some(state) = shape_results.get(name) else {
            continue;
        };
        println!("-- {} (persona: {}) --", name, shape.persona);
        if !state.globals.is_empty() {
            for (k, v) in &state.globals {
                println!("  {} = {}", k, v);
            }
        }
        if let Some(buckets) = &state.buckets {
            for b in buckets {
                let label = b.key_label();
                let computed = serde_json::to_string(&b.computed).unwrap_or_default();
                println!("  [{}] rows={} {}", label, b.rows.len(), computed);
            }
        } else if state.globals.is_empty() {
            println!("  {} row(s)", state.rows.len());
        }
        println!();
    }
    Ok(())
}

fn build_page(mapping: &MappingFile, shape_results: &HashMap<String, AggState>) -> serde_json::Value {
    let mut components = Vec::new();
    for (section_name, section) in &mapping.sections {
        let Some(state) = shape_results.get(&section.shape) else {
            continue;
        };
        let data = aggregate::to_json_summary(state);
        let mut comp = json!({
            "id": section_name,
            "shape": section.shape,
            "component": section.component,
            "config": section.config,
            "data": data,
        });
        if let Some(sec) = &section.secondary {
            comp["secondary"] = json!({ "component": sec.component, "config": sec.config });
        }
        components.push(comp);
    }
    json!({
        "title": mapping.mapping,
        "slug": mapping.output.slug,
        "folder": mapping.output.folder,
        "mode": mapping.output.mode,
        "components": components,
    })
}

fn write_file(mapping: &MappingFile, connector_dir: &Path, page: &serde_json::Value) -> Result<()> {
    let out_dir = connector_dir.join("output");
    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join(format!("{}.yaml", mapping.output.slug));
    let yaml = serde_yaml::to_string(page)?;
    fs::write(&out_path, yaml)?;
    println!("wrote {}", out_path.display());
    Ok(())
}

/// Stub: the curata MCP/API write integration isn't wired up yet. Prints the
/// page YAML that would be sent so the pull -> transform -> aggregate ->
/// output pipeline is inspectable end to end without it.
fn render_curata(
    mapping: &MappingFile,
    shape_results: &HashMap<String, AggState>,
    dry_run: bool,
    _force: bool,
    host: &HostConfig,
) -> Result<()> {
    let page = build_page(mapping, shape_results);
    let yaml = serde_yaml::to_string(&page)?;

    if dry_run {
        println!("\n--- dry run: would upsert curata page '{}' ---\n", mapping.output.slug);
        println!("{}", yaml);
        return Ok(());
    }

    if host.curata_url.is_none() || host.curata_token.is_none() {
        println!(
            "\nno curata_url/curata_token in ~/.kazam/connect.yaml - printing the generated page \
             YAML instead of writing it:\n"
        );
        println!("{}", yaml);
        return Ok(());
    }

    // TODO: POST to curata's write_page MCP/API endpoint once available.
    println!(
        "\ncurata write is not implemented yet - printing the generated page YAML for '{}':\n",
        mapping.output.slug
    );
    println!("{}", yaml);
    Ok(())
}
