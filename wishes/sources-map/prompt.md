# Source-of-Truth Mapping

You are mapping source code files to documentation pages. Each kazam page can declare `freshness.sources_of_truth` - a list of source files that, when changed, should trigger a doc review.

## Inputs

You need:
1. **Site directory** - the kazam site with YAML pages
2. **Source repos** - one or more code repositories to map from
3. **Repo prefix map** - how GitHub URLs map to local paths (ex. `https://github.com/org/repo/ → ~/repos/repo`)

## Process

### Step 1: Inventory the docs

Read each YAML page in the site. For each, note:
- File path
- Title
- Current `sources_of_truth` (if any)
- What the page is about (from components/content)

Use `kazam` MCP tools (`list_pages`, `read_page`) or read files directly. If `script.py` output is available, use that JSON summary instead of reading every file.

### Step 2: Scan source repos

For each source repo:
- Check for `.kazam/ctx/anatomy.tsv` - if it exists, start there (it's a compact index of directories and files with descriptions)
- Drill into `.kazam/ctx/anatomy/<dir>.tsv` for detail on specific directories
- If no anatomy files exist, use `find` or `ls` to understand the structure

Build a mental model of what each directory/file does and what documentation topic it would be a "source of truth" for.

### Step 3: Map pages to sources

For each documentation page, identify which source files define the behavior it documents:

**Good sources of truth:**
- Config files that define parameters the docs reference
- API endpoint handlers for API documentation pages
- Template files that define deployment artifacts
- Service entry points for feature documentation
- Schema/model files for data reference pages
- Connector/integration providers for integration setup guides

**Not good sources of truth:**
- Test files (they verify, not define)
- Internal utilities (implementation detail, not documented behavior)
- Build scripts (unless the docs are about the build process)
- Files so generic they'd trigger false drift signals

**Specificity guidelines:**
- Point at the most specific file that defines documented behavior
- For directories, prefer specific files over whole directories when the page is about one thing
- For overview pages covering a whole subsystem, a directory pointer is fine
- Each source should be something where `git log --since=<date> -- <path>` returning commits would be a meaningful signal

### Step 4: Output

For each page, output the updated `sources_of_truth` block as YAML. Format:

```yaml
# <page_path>  -  <page_title>
sources_of_truth:
- label: <descriptive label of what this source defines>
  href: <full GitHub URL>
```

Use full GitHub URLs: `https://github.com/<org>/<repo>/<path>`

Group output by section (environments, scanners, workflows, etc.) for readability.

## Tips

- Labels should describe WHAT the source defines for this doc, not just the file name
  - Good: "CloudFormation IAM role and policies"
  - Bad: "parent_template.yaml.j2"
- External documentation URLs (ex. Docker Hub docs, vendor docs) are valid sources of truth but won't be checked by `kazam freshness drift`
- Pages that are pure product overviews (no specific code backing) can point at the repo root - that's fine
- Normalize all hrefs to full GitHub URLs for consistency
