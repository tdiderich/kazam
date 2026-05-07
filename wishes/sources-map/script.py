#!/usr/bin/env python3
"""
Pre-process a kazam site for source-of-truth mapping.

Reads all YAML pages and extracts title + current sources_of_truth,
outputting a compact JSON summary that an LLM can use to generate
updated mappings without reading every page file.

Usage:
  python3 script.py <site_dir> [--repo PREFIX=LOCAL ...]
  python3 script.py ~/repos/maze-docs --repo "https://github.com/maze/atlas_universe/=~/repos/atlas_universe"
"""

import json
import os
import sys
from pathlib import Path

import yaml


def walk_pages(site_dir):
    """Walk YAML pages, extract title and sources_of_truth."""
    pages = []
    site = Path(site_dir)

    for p in sorted(site.rglob("*.yaml")):
        # Skip _site, .kazam, kazam.yaml, 404.yaml
        rel = p.relative_to(site)
        parts = rel.parts
        if any(part.startswith('.') or part == '_site' for part in parts):
            continue
        if p.name in ('kazam.yaml', '404.yaml'):
            continue

        try:
            with open(p) as f:
                data = yaml.safe_load(f)
        except Exception:
            continue

        if not isinstance(data, dict):
            continue

        title = data.get('title', p.stem)
        freshness = data.get('freshness', {})
        if isinstance(freshness, str):
            freshness = {}

        sources = []
        raw_sources = freshness.get('sources_of_truth', []) or []
        for s in raw_sources:
            if isinstance(s, str):
                sources.append({'href': s, 'label': s})
            elif isinstance(s, dict):
                sources.append({
                    'label': s.get('label', ''),
                    'href': s.get('href', ''),
                })

        pages.append({
            'path': str(rel),
            'title': title,
            'updated': freshness.get('updated'),
            'owner': freshness.get('owner'),
            'sources_of_truth': sources,
        })

    return pages


def scan_repo_anatomy(repo_path):
    """Read .kazam/ctx/anatomy.tsv if it exists, return summary."""
    anatomy_file = Path(repo_path) / '.kazam' / 'ctx' / 'anatomy.tsv'
    if not anatomy_file.exists():
        return None

    lines = anatomy_file.read_text().strip().split('\n')
    return {
        'path': str(repo_path),
        'anatomy': lines,
    }


def main():
    if len(sys.argv) < 2:
        print("Usage: script.py <site_dir> [--repo PREFIX=LOCAL ...]", file=sys.stderr)
        sys.exit(1)

    site_dir = sys.argv[1]

    # Parse --repo flags
    repos = []
    i = 2
    while i < len(sys.argv):
        if sys.argv[i] == '--repo' and i + 1 < len(sys.argv):
            parts = sys.argv[i + 1].split('=', 1)
            if len(parts) == 2:
                local = parts[1].replace('~', os.path.expanduser('~'))
                repos.append({'prefix': parts[0], 'local': local})
            i += 2
        else:
            i += 1

    pages = walk_pages(site_dir)

    # Scan repo anatomy files
    repo_summaries = []
    for repo in repos:
        summary = scan_repo_anatomy(repo['local'])
        if summary:
            summary['prefix'] = repo['prefix']
            repo_summaries.append(summary)

    output = {
        'site_dir': site_dir,
        'page_count': len(pages),
        'pages': pages,
        'repos': repo_summaries,
    }

    json.dump(output, sys.stdout, indent=2)
    print()


if __name__ == '__main__':
    main()
