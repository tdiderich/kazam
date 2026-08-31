# ICP Analysis Prompt

You have a CSV at `scripts/icp-data.csv` with customers, late-stage deals, and closed-lost deals. Each row has company firmographics from HubSpot + Apollo enrichment.

## Your task

Analyze this data and update the kazam YAML page at the path specified in the refresh block with a data-driven ICP. Structure your analysis as:

### 1. Customer DNA (P0 - highest confidence)
From existing customers, identify:
- **Company size sweet spot**: employee count range and median
- **Industry clusters**: which industries appear most
- **Geography**: where customers are concentrated
- **Tech stack signals**: common technologies (from Apollo)
- **Revenue/funding profile**: typical company stage and size
- What makes these companies similar? What's the archetype?

### 2. Pipeline validation (P1 - late-stage deals)
From late-stage deals (Business Validation, Negotiation, Closed Won):
- Do they match the customer DNA or diverge?
- Any new segments emerging in the pipeline?
- Deal size patterns

### 3. Closed-lost patterns (P2 - lighter review)
From closed-lost deals:
- Common loss reasons
- Are there company profiles that consistently lose? (wrong size, wrong industry, wrong stage)
- Any "near misses" worth understanding?

### 4. Updated ICP definition
Synthesize into:
- **Tier 1**: Companies that look like our customers (define the profile)
- **Tier 2**: Companies that look like our late-stage pipeline
- **Tier 3**: Worth pursuing but lower confidence
- **Disqualifiers**: Patterns from closed-lost that signal bad fit

Keep the existing page structure (kazam YAML with components) and replace the content with data-backed definitions. Use `type: table` with `columns` (key/label) and `rows` (key-value maps) for data tables.
