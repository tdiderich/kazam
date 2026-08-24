# kazam connect - Verb Catalog & Runtime Spec

Declarative YAML verbs that `kazam connect` interprets in Rust. No scripting
language. Agents edit YAML, the binary executes it.

Validated against: runZero exposure connector + Maze investigations connector.

## Design Principles

1. **Closed verb set** - every operation the runtime can perform is a named verb.
   No escape hatches, no embedded expressions, no eval.
2. **YAML-native** - every verb has a fixed YAML schema a Rust `serde` deserializer
   can parse unambiguously. No free-form strings that need a secondary parser.
3. **Three layers** - pulls, transforms, aggregation - executed in that order.
   Transforms run once per record after pull. Aggregation runs on the full dataset.
4. **Deterministic** - same input data + same mapping file = same output. Always.

---

## Layer 1: Pull Verbs (5 verbs)

These describe how to get data from the API. Executed by Rust's HTTP client.

### `request`

The HTTP call itself.

```yaml
pulls:
  assets:
    request:
      method: GET
      path: /export/org/assets.json
      params:
        fields: id,alive,type,os,risk_rank,risk
        page_size: 1000
```

For POST bodies:
```yaml
pulls:
  investigations:
    request:
      method: POST
      path: /v1/investigations/search
      body:
        updated_from: "{{last_sync}}"
        limit: 100
```

**Rust complexity**: easy. `reqwest` handles this natively.

### `paginate`

How to follow pages. Three patterns cover all known APIs.

```yaml
# Keyset (runZero): response contains next_key, pass as start_key param
paginate:
  style: keyset
  next_from: .next_key
  send_as: start_key

# Cursor (Maze): response has next_cursor, keep going while has_more is true
paginate:
  style: cursor
  next_from: .next_cursor
  while: .has_more

# Offset: increment page number
paginate:
  style: offset
  param: page
  start: 1
```

**Rust complexity**: easy. Finite state machine with 3 variants.

### `collect`

Where records live in the response JSON.

```yaml
# Nested key (paged responses)
collect: .assets[]

# Root array (bare responses)
collect: .[]

# Conditional: paged wraps in key, bare returns root array
collect:
  paged: .assets[]
  bare: .[]
```

**Rust complexity**: easy. `serde_json` pointer + array iteration.

### `auth`

Declared once in `source:`, applied to every pull.

```yaml
source:
  auth:
    # Bearer token
    type: bearer
    value: "{{RZ_EXPORT_TOKEN}}"

    # API key in header
    type: api_key
    header: X-API-Key
    value: "{{MAZE_API_KEY}}"

    # OAuth2 client credentials
    type: oauth2
    token_url: https://auth.example.com/token
    client_id: "{{CLIENT_ID}}"
    client_secret: "{{CLIENT_SECRET}}"
    scope: read
```

**Rust complexity**: easy. Three auth patterns, all well-supported by reqwest.

### `rate_limit`

How to handle rate limiting.

```yaml
rate_limit:
  strategy: retry_after   # honor Retry-After header on 429
  max_retries: 3
  backoff: exponential     # 1s, 2s, 4s

# Or fixed delay between requests
rate_limit:
  strategy: fixed_delay
  delay_ms: 200
```

**Rust complexity**: easy. `tokio::time::sleep` + retry loop.

---

## Layer 2: Transform Verbs (8 verbs)

These normalize raw API records into clean, typed rows before aggregation.
Declared per-pull in a `transforms:` block. Executed once per record.

All transform verbs support nested array paths (`.parent[].field`). The runtime
iterates each array element and applies the transform. This is critical for APIs
like Maze where fields needing cleanup live inside nested arrays.

### `coerce`

Force a field to a specific type.

```yaml
transforms:
  - coerce: .service_protocol
    to: list
    # string "http" -> ["http"]
    # list ["http", "tls"] -> ["http", "tls"] (no-op)

  - coerce: .risk_rank
    to: int
    # "3" -> 3, already int -> no-op

  - coerce: .vulnerability_cvss3_base_score
    to: float
```

Types: `string`, `int`, `float`, `bool`, `list`.

- `to: list` on a string wraps it in `["value"]`; on a list it's a no-op
- `to: string` on a list joins with `", "` (configurable via `join:`)
- `to: int` / `to: float` parse from string, pass through numbers
- `to: bool` handles `true/false`, `1/0`, `yes/no`

**Rust complexity**: easy. Match on `serde_json::Value` variant, coerce.

**Used by**: runzero (service_protocol string|list), general (API type mismatches)

### `default`

Fill missing or empty values.

```yaml
transforms:
  - default: .type
    value: "unknown"
    when: empty    # "", null, or missing

  - default: .os_version
    value: "undetected"
    when: missing  # null or absent only, preserves ""
```

`when`: `empty` (default) = null, missing, or `""`; `missing` = null or absent only.

**Rust complexity**: easy. Null/empty check + insert.

**Used by**: runzero (type/os gaps), general

### `rename`

Rename a field. Simple but necessary for ergonomics.

```yaml
transforms:
  - rename: .vulnerability_cvss3_base_score
    to: .cvss
```

**Rust complexity**: trivial. Remove key, insert under new name.

**Used by**: both (long vendor field names -> short working names)

### `lowercase`

Normalize string case for consistent bucketing.

```yaml
transforms:
  - lowercase: .vulnerability_severity
  - lowercase: .risk
```

**Rust complexity**: trivial. `.to_lowercase()`.

**Used by**: runzero (severity strings come as "Critical" not "critical"), general

### `strip`

Remove a prefix, suffix, or regex-matched substring from a string.

```yaml
transforms:
  # Strip everything before the last /
  - strip: .asset_name
    before_last: "/"

  # Strip @suffix
  - strip: .asset_name
    after_first: "@"

  # Strip :suffix
  - strip: .asset_name
    after_first: ":"

  # Strip a prefix
  - strip: .firmware_version
    prefix: "v"

  # Strip a suffix
  - strip: .hostname
    suffix: ".local"

  # Nested array path: applies to each element
  - strip: .related_scanner_findings[].asset_name
    before_last: "/"
```

Modes: `prefix`, `suffix`, `before_last`, `after_first`, `pattern` (regex).

**Nested paths**: any transform verb can target a field inside a nested array
using `[]` syntax (`.parent[].child`). The runtime iterates each array element
and applies the transform to the named field. This runs at pull time before
aggregation, so expanded rows already have clean values.

**Rust complexity**: easy for prefix/suffix/before_last/after_first. Medium for regex (needs `regex` crate). Nested path iteration adds ~20 lines.

**Used by**: maze (asset_name cleaning inside related_scanner_findings[]), general

### `regex`

Extract a value from a string via capture group.

```yaml
transforms:
  - regex: .firmware_string
    pattern: "v(\\d+\\.\\d+\\.\\d+)"
    capture: 1
    into: .firmware_version
```

If no match, the target field is set to null (or `default:` if specified).

**Rust complexity**: medium. `regex` crate, compiled once per transform.

**Used by**: general (version extraction, embedded data parsing)

### `epoch`

Convert Unix epoch timestamps to ISO 8601 strings or relative durations.

```yaml
transforms:
  - epoch: .last_seen
    to: iso8601

  - epoch: .last_seen
    to: age_days    # derives days since epoch, stores as int
    into: .days_since_seen

  - epoch: .eol_os
    to: iso8601
    zero_means: null  # epoch 0 -> null (common sentinel)
```

Units: `seconds` (default), `milliseconds`, `nanoseconds` (for newest_mac_age).

**Rust complexity**: easy. `chrono::DateTime` from timestamp.

**Used by**: runzero (all timestamps are epoch seconds, newest_mac_age is nanoseconds)

### `flatten`

Flatten a nested object into dot-separated top-level fields.

```yaml
transforms:
  - flatten: .attributes
    separator: "."
    prefix: "attr"
    # {attributes: {env: "prod", tier: "1"}} -> {attr.env: "prod", attr.tier: "1"}
```

**Rust complexity**: easy. Recursive walk of `serde_json::Value::Object`.

**Used by**: runzero (attributes, tags, services objects), general

---

## Layer 3: Aggregation Verbs (10 verbs)

These operate on the full transformed dataset. Declared in `shapes[].aggregate`.
This is the existing verb set from the mapping skill, now with precise YAML schemas
for Rust interpretation.

### `expand`

Flatten a nested array into rows (one row per array element, parent fields preserved).

```yaml
- expand: .related_scanner_findings[]
```

Produces one row per element. Each row inherits all parent fields plus the
element's fields merged in.

**Rust complexity**: medium. Clone parent row per element, merge fields.

**Used by**: maze (related_scanner_findings)

### `filter`

Remove rows not matching a condition.

```yaml
- filter:
    where: .service_transport = "tcp"

- filter:
    where: .risk_rank >= 3

- filter:
    where: .vulnerability_severity
    in: [critical, high]
```

Operators: `=`, `!=`, `>`, `>=`, `<`, `<=`, `in`, `not_in`, `is_empty`, `not_empty`.

**Rust complexity**: easy. Expression evaluator over `serde_json::Value`.

**Used by**: both

### `bucket`

Group rows by one or more fields.

```yaml
- bucket:
    by: [.risk]
    ordered: [critical, high, medium, low, info, none]

- bucket:
    by: [.service_port, .service_protocol]
```

`ordered:` specifies the bucket output order (missing values go to an "other" bucket).
Without `ordered:`, buckets appear in first-seen order.

**Rust complexity**: easy. `HashMap<Vec<Value>, Vec<Row>>`.

**Used by**: both

### `tally`

Count rows within each bucket (or globally).

```yaml
# Count all rows in bucket
- tally:
    all: true
    as: asset_count

# Count rows matching a condition
- tally:
    where: .software_cpe23
    not_empty: true
    as: cpe_present

# Count distinct values of a field
- tally:
    distinct: .vulnerability_asset_id
    as: affected_assets
```

**Rust complexity**: easy. Counter with optional filter/distinct.

**Used by**: both (heavily)

### `derive`

Compute a new field from existing aggregation results.

```yaml
- derive:
    name: noise_pct
    expr: noise / total * 100

- derive:
    name: at_risk
    expr: sum(asset_count) where bucket in [critical, high]
```

Expression language is arithmetic only: `+`, `-`, `*`, `/`, `%`, plus
`sum()`, `count()`, `min()`, `max()`, `avg()` aggregate functions.
References are to names defined by prior `tally` or `derive` steps.

**Rust complexity**: medium. Need a small expression evaluator. Can use a
simple recursive-descent parser since the grammar is fixed (no variables,
no function definitions, just arithmetic on named values).

**Used by**: both

### `compare`

Compute a delta or ratio between two fields.

```yaml
- compare:
    a: scanner_count
    b: maze_count
    op: subtract    # a - b
    as: shift

- compare:
    a: maze_actionable
    b: scanner_actionable
    op: ratio       # a / b
    as: reduction_ratio
```

Ops: `subtract`, `ratio`, `percent_change`.

**Rust complexity**: easy. Two value lookups + arithmetic.

**Used by**: maze (severity_shift, noise_reduction)

### `rank`

Sort rows or buckets.

```yaml
- rank:
    by: .cvss3_base_score
    direction: desc

# Multi-field sort
- rank:
    by: [.risk_rank, .vuln_density]
    direction: [desc, desc]

# Categorical sort (specific order)
- rank:
    by: .vulnerability_severity
    order: [critical, high, medium, low, info]
```

**Rust complexity**: easy. `sort_by` with comparator chain.

**Used by**: both

### `take`

Limit output to top N rows.

```yaml
- take: 10
```

**Rust complexity**: trivial. `.truncate(n)`.

**Used by**: both

### `distinct`

Unique values of a field.

```yaml
- distinct: .scanner
```

**Rust complexity**: easy. `HashSet`.

**Used by**: maze (scanner_coverage)

### `clean`

**Deprecated in favor of transform verbs.** The original `clean` verb tried to do
transforms inside aggregation blocks. With the transform layer, `clean` operations
should be expressed as `strip`, `lowercase`, `coerce`, etc. in the `transforms:` block.

Kept for backward compatibility in human-readable aggregate pseudo-code only.
The runtime ignores `clean` lines and expects the equivalent transforms to be declared.

---

## Coverage Matrix

| Verb | Layer | runZero | Maze | General |
|------|-------|---------|------|---------|
| **Pull** | | | | |
| request | pull | GET + params | POST + body | both patterns |
| paginate | pull | keyset | cursor | + offset |
| collect | pull | conditional (paged/bare) | nested key | both |
| auth | pull | bearer | api_key | + oauth2 |
| rate_limit | pull | proportional | retry_after | both |
| **Transform** | | | | |
| coerce | transform | service_protocol str→list | - | type mismatches |
| default | transform | type/os empty gaps | - | missing fields |
| rename | transform | long field names | long field names | ergonomics |
| lowercase | transform | severity/risk case | - | string normalization |
| strip | transform | - | asset_name cleaning (nested path) | prefix/suffix |
| regex | transform | - | - | version extraction |
| epoch | transform | all timestamps | - | epoch→iso |
| flatten | transform | attributes/tags objects | - | nested objects |
| **Aggregation** | | | | |
| expand | agg | - | related_scanner_findings | nested arrays |
| filter | agg | transport, port, severity | severity filter | conditions |
| bucket | agg | risk, port, vendor | severity_level | grouping |
| tally | agg | counts, distinct, conditional | counts, distinct | counting |
| derive | agg | pct, avg, density | noise_pct, reduction | arithmetic |
| compare | agg | - | scanner vs maze | deltas/ratios |
| rank | agg | by count, score, density | by severity, cve | sorting |
| take | agg | top 10-20 | top 10-20 | limiting |
| distinct | agg | - | scanner list | unique values |
| clean | agg (deprecated) | - | - | use transforms |

**Total: 23 verbs** (5 pull + 8 transform + 10 aggregation)

---

## Gaps & Honest Assessment

### No escape hatches needed

Both mapping files are fully expressible with these 23 verbs. No connector needed
conditional logic (if/else), loops, or custom functions. The transform layer handles
every data normalization case that previously required inline `clean` pseudo-code.

### Edge cases covered

1. **service_protocol string|list** → `coerce .service_protocol to: list`
2. **Maze asset_name cleaning** → chain of `strip` transforms
3. **runZero epoch timestamps** → `epoch .last_seen to: iso8601`
4. **runZero newest_mac_age nanoseconds** → `epoch .newest_mac_age to: iso8601 unit: nanoseconds`
5. **Missing device types** → `default .type value: "unknown"`
6. **Case inconsistency** → `lowercase .vulnerability_severity`
7. **Denormalized exports** → no special handling needed; flat records work as-is

### Potential future gaps

These might surface with more connectors:

- **Join across pulls** - merging assets + vulnerabilities by asset_id.
  Current design: denormalized exports avoid this (runZero). Maze uses a single
  pull. If a connector needs client-side joins, add a `join` verb.
- **Conditional transforms** - "if field X exists, coerce it; otherwise skip."
  Current: `coerce` on a missing field is a no-op. May need `when:` conditions.
- **Multi-value regex** - extracting multiple capture groups into multiple fields.
  Current: `regex` captures one group. Could extend with `captures:` map.
- **Nested aggregation** - bucket within bucket. Neither connector needs it.
  If needed, add `sub_bucket` or allow nested `aggregate:` blocks.

### Rust implementation estimate

| Complexity | Verbs | Estimate |
|-----------|-------|----------|
| Trivial | take, rename, lowercase, distinct | < 1 day |
| Easy | request, auth, collect, rate_limit, coerce, default, flatten, filter, bucket, tally, compare, rank | 3-5 days |
| Medium | paginate, strip, regex, epoch, expand, derive | 3-4 days |

**Total**: ~2 weeks for a working interpreter. The expression evaluator for `derive`
is the hardest single piece.

---

## Config Resolution

Three-level resolution, lowest wins: shell environment → `~/.kazam/connect.yaml` → connector `.env`.

### Host-level config (`~/.kazam/connect.yaml`)

Shared across all connectors on this machine.

```yaml
curata_url: https://curata.ai       # or http://localhost:3000 for self-hosted/OSS
curata_token: "ct_..."               # curata API auth
default_target: curata               # curata | terminal | both
```

### Connector-level secrets (`connectors/<vendor>/.env`, gitignored)

Per-vendor API credentials. Template variables in `source.auth.value` resolve
from this file first, then host config, then shell env.

```
RZ_EXPORT_TOKEN=ET:abc123...
RUNZERO_BASE_URL=https://console.runzero.com/api/v1.0
```

```
MAZE_API_KEY=mk_...
MAZE_API_URL=https://api.maze.security
```

### Resolution order

1. Shell environment (`$RZ_EXPORT_TOKEN`)
2. `~/.kazam/connect.yaml` (host-level)
3. `connectors/<vendor>/.env` (connector-level)

Connector `.env` wins. `{{VAR}}` template syntax in mapping files resolves
through this chain. Missing required vars = hard error before any HTTP call.

---

## Output Block

Every mapping file declares where results go.

```yaml
output:
  target: curata          # curata | terminal | both | file
  slug: runzero-exposure  # curata page slug (required for curata target)
  mode: upsert            # upsert | create | update
  folder: connectors      # curata folder to place page in
```

### Targets

| Target | Behavior |
|--------|----------|
| `curata` | Write page via curata MCP/API. Requires `curata_url` + `curata_token` in config. |
| `terminal` | Render to stdout as formatted table/dashboard. No external writes. |
| `both` | Write to curata AND render to terminal. |
| `file` | Write page YAML to `connectors/<vendor>/output/<slug>.yaml`. No API calls. |

CLI override: `kazam connect runzero --target terminal` overrides the mapping file's target.

### Curata instance routing

Three modes based on `curata_url` in `~/.kazam/connect.yaml`:

| Mode | curata_url | Auth |
|------|-----------|------|
| Cloud | `https://curata.ai` | `curata_token` (required) |
| Self-hosted | `http://localhost:3000` (or any non-curata.ai URL) | `curata_token` (required) |
| Local/file | omitted, or `--target file` | None. Writes YAML to disk. |

Default from config. `--target file` always writes to disk regardless of config.

---

## State Tracking

Per-connector state file at `connectors/<vendor>/.state.yaml` (gitignored).
Managed by the runtime, not hand-edited.

```yaml
last_sync: 2026-08-24T18:00:00Z    # timestamp of last successful sync
content_hash: 2f6001fb...            # hash of last written page content
page_created: true                   # whether the curata page exists
pull_counts:                         # record counts from last sync
  assets: 30
  services: 325
  software: 45
  vulnerabilities: 41
```

### How state is used

- `{{last_sync}}` in pull request bodies resolves from `.state.yaml`.
  First run (no state file) uses epoch 0 or omits the field.
- `content_hash` enables skip-on-no-change: if aggregation produces the same
  hash as last run, skip the curata write. `--force` overrides.
- `page_created` tracks whether `mode: upsert` should create or update.
- `pull_counts` is informational. Shown in `kazam connect status`.

---

## CLI Surface

```
kazam connect <vendor>               # run the default mapping for this vendor
kazam connect <vendor> --dry-run     # pull + transform + aggregate, print results, don't write
kazam connect <vendor> --target terminal  # override output target
kazam connect <vendor> --force       # write even if content unchanged
kazam connect status                 # show all connectors + last sync times
kazam connect status <vendor>        # show detailed state for one connector
```

---

## Retrofitted Mapping Files

See:
- `connectors/runzero/exposure.map.yaml` - updated with structured pull/transform/output blocks
- `connectors/maze/investigations.map.yaml` - new, full Maze mapping with verb catalog

Both files use the exact YAML schemas defined above. A Rust `serde` deserializer
can parse them without ambiguity.
