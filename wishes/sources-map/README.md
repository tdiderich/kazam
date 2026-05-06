# sources-map

Maps source code repositories to kazam page `sources_of_truth` metadata. Scans repo anatomy files or directory trees, reads page content, then uses an LLM to produce specific file-level mappings.

## Quickstart

```bash
kazam wish init sources-map
```

Then run the prompt with your AI agent, providing:
- The kazam site directory
- One or more source repo paths
- Repo prefix mappings (GitHub URL → local path)

## What it produces

Updated `freshness.sources_of_truth` blocks for each page, with:
- Specific file paths instead of directory-level pointers
- Descriptive labels explaining what each source file defines
- Full GitHub URLs (consistent, linkable)

## Usage with kazam freshness drift

After mapping sources, configure `drift.repos` in your `kazam.yaml`:

```yaml
drift:
  repos:
    - prefix: "https://github.com/your-org/your-repo/"
      local: "~/repos/your-repo"
```

Then run `kazam freshness drift` to detect when source code changes haven't been reflected in docs.

## Pre-processing with script.py

To avoid feeding every YAML page to the LLM, run the bundled script first:

```bash
python3 script.py <site_dir> --repo "https://github.com/org/repo/=~/repos/repo"
```

Pipe the JSON output to your agent alongside `prompt.md`.
