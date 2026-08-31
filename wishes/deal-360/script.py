#!/usr/bin/env python3
"""
Pull active deal data from HubSpot for Deal 360 pages.

Fetches all open deals (not closed-won or closed-lost) with associated
companies, contacts, and owner info. Outputs structured JSON that an
agent enriches with Attention call intelligence and Slack channel activity.

Env vars: HUBSPOT_API_TOKEN
Output:
  scripts/deal-360-data.json  - structured deal data
  scripts/deal-360-prompt.md  - analysis prompt for the agent
"""

# ── Customization block ──────────────────────────────────────────────────────
#
# PIPELINE_NAME: Which HubSpot pipeline to pull deals from.
#   Set to None to pull from all pipelines.
#
# STAGES_TO_INCLUDE: Stage label prefixes to include. Empty list = all open stages.
#   Example: ["S3", "S4", "S5", "S6"] to only show qualified+ deals.
#
# SLACK_CHANNEL_PREFIX: Prefix for deal-specific Slack channels.
#   Many orgs auto-create channels like #int-deal-<company>.
#
# MAX_DEALS: Cap on deals to fetch. Set higher for larger pipelines.

PIPELINE_NAME = None
STAGES_TO_INCLUDE = []
SLACK_CHANNEL_PREFIX = "int-deal-"
MAX_DEALS = 200

# ────────────────────────────────────────────────────────────────────────────

import json
import os
import sys
import time
from pathlib import Path

import requests


def load_dotenv(path):
    if not os.path.exists(path):
        return
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, val = line.partition("=")
            os.environ.setdefault(key.strip(), val.strip().strip("'\""))


load_dotenv(os.path.join(os.path.dirname(__file__), "..", ".env"))

HUBSPOT_KEY = os.environ.get("HUBSPOT_API_TOKEN", "") or os.environ.get("HUBSPOT_API_KEY", "")
if not HUBSPOT_KEY:
    sys.exit("HUBSPOT_API_TOKEN not set - add it to .env")

HS_BASE = "https://api.hubapi.com"
HS_HEADERS = {"Authorization": f"Bearer {HUBSPOT_KEY}", "Content-Type": "application/json"}

DEAL_PROPS = [
    "dealname", "dealstage", "amount", "closedate", "pipeline",
    "hubspot_owner_id", "description", "hs_lastmodifieddate",
    "hs_is_closed_won", "hs_is_closed",
    "champion", "decision_maker_in_deal_",
    "loss_reason", "closed_lost_reason", "closed_won_reason",
]

COMPANY_PROPS = [
    "name", "domain", "industry", "numberofemployees", "annualrevenue",
    "city", "state", "country", "description", "lifecyclestage",
    "customer_health", "health_summary",
]

CONTACT_PROPS = [
    "firstname", "lastname", "email", "jobtitle", "phone",
]


def hs_get(path, params=None):
    r = requests.get(f"{HS_BASE}{path}", headers=HS_HEADERS, params=params or {})
    r.raise_for_status()
    return r.json()


def hs_post(path, body):
    r = requests.post(f"{HS_BASE}{path}", headers=HS_HEADERS, json=body)
    r.raise_for_status()
    return r.json()


def get_stage_map():
    pipelines = hs_get("/crm/v3/pipelines/deals")
    id_to_label = {}
    label_to_id = {}
    for pipeline in pipelines.get("results", []):
        for stage in pipeline.get("stages", []):
            label = stage.get("label", "")
            sid = stage.get("stageId") or stage.get("id", "")
            id_to_label[sid] = label
            label_to_id[label] = sid
    return label_to_id, id_to_label


def get_owners():
    owners = {}
    data = hs_get("/crm/v3/owners", {"limit": 200})
    for owner in data.get("results", []):
        oid = owner.get("id")
        owners[oid] = {
            "name": f"{owner.get('firstName', '')} {owner.get('lastName', '')}".strip(),
            "email": owner.get("email", ""),
        }
    return owners


def search_open_deals(label_to_id, id_to_label):
    stage_ids = []
    if STAGES_TO_INCLUDE:
        for label, sid in label_to_id.items():
            if any(label.lower().startswith(p.lower()) for p in STAGES_TO_INCLUDE):
                stage_ids.append(sid)
    else:
        closed_labels = {"closed - won", "closed - lost", "closed won", "closed lost"}
        for label, sid in label_to_id.items():
            if label.lower() not in closed_labels and "lost" not in label.lower() and "won" not in label.lower():
                stage_ids.append(sid)

    if not stage_ids:
        print("  No matching stages found - check STAGES_TO_INCLUDE config")
        return []

    print(f"  Searching {len(stage_ids)} open stages...")

    all_deals = []
    after = None
    while True:
        body = {
            "filterGroups": [{"filters": [
                {"propertyName": "dealstage", "operator": "IN", "values": stage_ids},
            ]}],
            "properties": DEAL_PROPS,
            "sorts": [{"propertyName": "hs_lastmodifieddate", "direction": "DESCENDING"}],
            "limit": min(MAX_DEALS - len(all_deals), 100),
        }
        if after:
            body["after"] = after
        data = hs_post("/crm/v3/objects/deals/search", body)
        all_deals.extend(data.get("results", []))
        paging = data.get("paging", {}).get("next", {})
        after = paging.get("after")
        if not after or len(all_deals) >= MAX_DEALS:
            break
    return all_deals[:MAX_DEALS]


def get_associations(deal_id, to_type):
    try:
        data = hs_get(f"/crm/v3/objects/deals/{deal_id}/associations/{to_type}")
        return [r.get("id") for r in data.get("results", []) if r.get("id")]
    except Exception:
        return []


def get_objects_batch(object_type, ids, props):
    objects = {}
    batch_size = 100
    for i in range(0, len(ids), batch_size):
        batch = list(ids)[i:i + batch_size]
        body = {
            "inputs": [{"id": oid} for oid in batch],
            "properties": props,
        }
        data = hs_post(f"/crm/v3/objects/{object_type}/batch/read", body)
        for result in data.get("results", []):
            objects[result["id"]] = result.get("properties", {})
    return objects


def slugify(name):
    return name.lower().replace(" ", "-").replace("&", "and").replace("'", "").replace(",", "")


def main():
    print("Building stage map...")
    label_to_id, id_to_label = get_stage_map()
    print(f"  {len(label_to_id)} stages across all pipelines")

    print("Fetching owners...")
    owners = get_owners()
    print(f"  {len(owners)} owners")

    print("Searching open deals...")
    deals_raw = search_open_deals(label_to_id, id_to_label)
    print(f"  Found {len(deals_raw)} open deals")

    print("Fetching associations...")
    all_company_ids = set()
    all_contact_ids = set()
    deal_companies = {}
    deal_contacts = {}

    for d in deals_raw:
        did = d["id"]
        cids = get_associations(did, "companies")
        deal_companies[did] = cids
        all_company_ids.update(cids)

        contact_ids = get_associations(did, "contacts")
        deal_contacts[did] = contact_ids
        all_contact_ids.update(contact_ids)

    print(f"  {len(all_company_ids)} unique companies, {len(all_contact_ids)} unique contacts")

    print("Fetching company details...")
    companies = get_objects_batch("companies", all_company_ids, COMPANY_PROPS) if all_company_ids else {}

    print("Fetching contact details...")
    contacts = get_objects_batch("contacts", all_contact_ids, CONTACT_PROPS) if all_contact_ids else {}

    deals_output = []
    for d in deals_raw:
        did = d["id"]
        props = d.get("properties", {})
        stage_id = props.get("dealstage", "")
        stage_label = id_to_label.get(stage_id, stage_id)

        owner_id = props.get("hubspot_owner_id", "")
        owner = owners.get(owner_id, {})

        company_ids = deal_companies.get(did, [])
        company = companies.get(company_ids[0], {}) if company_ids else {}
        company_name = company.get("name", "")
        domain = company.get("domain", "")

        deal_contact_ids = deal_contacts.get(did, [])
        deal_contacts_list = []
        for cid in deal_contact_ids:
            c = contacts.get(cid, {})
            deal_contacts_list.append({
                "name": f"{c.get('firstname', '')} {c.get('lastname', '')}".strip(),
                "email": c.get("email", ""),
                "title": c.get("jobtitle", ""),
                "phone": c.get("phone", ""),
            })

        health_summary = None
        if company.get("health_summary"):
            try:
                health_summary = json.loads(company["health_summary"])
            except (json.JSONDecodeError, TypeError):
                health_summary = company.get("health_summary")

        slack_channel_guess = f"#{SLACK_CHANNEL_PREFIX}{slugify(company_name)}" if company_name else None

        deals_output.append({
            "deal_id": did,
            "deal_name": props.get("dealname", ""),
            "stage": stage_label,
            "amount": props.get("amount"),
            "close_date": props.get("closedate"),
            "last_modified": props.get("hs_lastmodifieddate"),
            "description": props.get("description", ""),
            "champion": props.get("champion", ""),
            "decision_maker": props.get("decision_maker_in_deal_", ""),
            "owner": {
                "name": owner.get("name", ""),
                "email": owner.get("email", ""),
            },
            "company": {
                "name": company_name,
                "domain": domain,
                "industry": company.get("industry", ""),
                "employees": company.get("numberofemployees", ""),
                "revenue": company.get("annualrevenue", ""),
                "country": company.get("country", ""),
                "health": company.get("customer_health", ""),
                "health_summary": health_summary,
            },
            "contacts": deal_contacts_list,
            "slack_channel_guess": slack_channel_guess,
        })

    output = {
        "generated": time.strftime("%Y-%m-%d"),
        "deal_count": len(deals_output),
        "stage_map": {v: k for k, v in id_to_label.items()},
        "deals": deals_output,
    }

    scripts_dir = Path(__file__).parent
    json_path = scripts_dir / "deal-360-data.json"
    json_path.write_text(json.dumps(output, indent=2, default=str))

    stage_counts = {}
    for d in deals_output:
        stage_counts[d["stage"]] = stage_counts.get(d["stage"], 0) + 1

    prompt_path = scripts_dir / "deal-360-prompt.md"
    prompt_path.write_text(f"""# Deal 360 Analysis Prompt

You have HubSpot deal data at `scripts/deal-360-data.json` with {len(deals_output)} open deals.

Stage distribution: {json.dumps(stage_counts)}

## Your task

Build a Deal 360 page. For EACH deal in the JSON, create a rich dossier by combining three data sources:

### Step 1: Read the HubSpot data (already in JSON)
Each deal has: stage, amount, owner, close date, company info (industry, size, health), contacts (names, titles, emails), and a guessed Slack channel name.

### Step 2: Enrich with Attention call intelligence
For each deal, search Attention for calls mentioning the company name or deal name:
- Use `search_calls` with the company name as transcript search
- For deals with 1+ calls, use `ask_attention` (up to 25 call IDs) asking:
  "For this deal, summarize: (1) primary pain points and buying motivation, (2) competitive landscape - what else are they evaluating, (3) objections raised and how they were handled, (4) next steps and commitments, (5) risk signals (budget freeze, missing authority, timeline slip)"
- If no calls found, note "No call data available"

### Step 3: Enrich with Slack channel activity
For each deal, check if the guessed Slack channel exists (field: `slack_channel_guess`):
- Search Slack channels for the channel name
- If found, read the last 20 messages for recent activity summary
- Look for: blockers, technical scoping notes, feature requests, POV progress
- If no channel found, note "No Slack channel"

### Output format

Update the kazam YAML page with one section per deal, ordered by stage (latest stage first). Each deal section should have:

```yaml
- type: section
  heading: "Deal Name (Stage)"
  components:
    - type: table
      columns:
        - key: field
          label: ""
        - key: value
          label: ""
      rows:
        - field: Owner
          value: "name (email)"
        - field: Amount
          value: "$X"
        - field: Close Date
          value: "YYYY-MM-DD"
        - field: Company
          value: "name - industry, N employees, country"
        - field: Champion
          value: "name, title"
        - field: Health
          value: "High/Medium/Low (if customer)"
    - type: callout
      style: info
      title: "Call Intelligence"
      body: |
        (Attention summary: pain points, competition, objections, next steps, risks)
    - type: markdown
      body: |
        **Slack Activity**
        (Recent channel activity summary or "No Slack channel")
```

Group deals by stage with a stage header. Include a summary section at the top with pipeline health metrics (deals per stage, total pipeline value for S4+).
""")

    print(f"\nWrote:")
    print(f"  {json_path} ({len(deals_output)} deals)")
    print(f"  {prompt_path}")
    print(f"\nNext steps:")
    print(f"  1. Have an agent with Attention + Slack MCPs read deal-360-prompt.md")
    print(f"  2. The agent enriches each deal with call insights and Slack activity")
    print(f"  3. Agent updates the Deal 360 page with full dossiers")


if __name__ == "__main__":
    main()
