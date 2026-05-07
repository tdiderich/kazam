#!/usr/bin/env python3
"""
Pull ICP data from HubSpot + Apollo and write CSV + analysis prompt.

Sources:
  1. HubSpot customers (lifecycle_stage = customer) — P0
  2. HubSpot late-stage deals (by stage label prefix) — P1
  3. HubSpot closed-lost deals — P2
  4. Apollo org search — enrichment for each company

Uses deal stage LABELS (not IDs) because closed-lost stage IDs can be reused.

Env vars: HUBSPOT_API_TOKEN, APOLLO_API_KEY
Output:
  scripts/icp-data.json   — full structured data
  scripts/icp-data.csv    — flat CSV for agent review
  scripts/icp-prompt.md   — analysis prompt for the agent
"""

# ── Customization block ──────────────────────────────────────────────────────
#
# On first run, the script prints ALL stage labels from your HubSpot pipelines.
# Check that output and update the prefixes below to match your deal stages.
#
# LATE_STAGE_PREFIXES: lowercase prefix(es) that identify deals you'd consider
#   "in advanced evaluation" — e.g. business validation, negotiation, verbal commit.
#   The script matches any stage label that starts with one of these prefixes.
#
# CLOSED_LOST_PREFIXES: lowercase prefix(es) for closed-lost stage labels.
#   NOTE: The script also queries hs_is_closed_won=false as a fallback, so
#   this list mainly controls which stage IDs to use in alternate searches.
#   You may not need to change this.

LATE_STAGE_PREFIXES = ["s4", "s5", "s6", "s7", "closed -  won", "closed - won", "closed won"]
CLOSED_LOST_PREFIXES = ["closed - lost", "closed lost"]

# ────────────────────────────────────────────────────────────────────────────

import csv
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


# Load .env from site root (one level up from scripts/)
load_dotenv(os.path.join(os.path.dirname(__file__), "..", ".env"))

HUBSPOT_KEY = os.environ.get("HUBSPOT_API_TOKEN", "") or os.environ.get("HUBSPOT_API_KEY", "")
APOLLO_KEY = os.environ.get("APOLLO_API_KEY", "") or os.environ.get("APOLLO_API_TOKEN", "")

if not HUBSPOT_KEY:
    sys.exit("HUBSPOT_API_TOKEN not set — add it to .env")

HS_BASE = "https://api.hubapi.com"
HS_HEADERS = {"Authorization": f"Bearer {HUBSPOT_KEY}", "Content-Type": "application/json"}

AP_BASE = "https://api.apollo.io/api/v1"
AP_HEADERS = {"x-api-key": APOLLO_KEY, "Content-Type": "application/json", "Cache-Control": "no-cache"}

COMPANY_PROPS = [
    "name", "domain", "industry", "numberofemployees", "annualrevenue",
    "city", "state", "country", "description", "lifecyclestage",
    "hs_num_open_deals", "total_revenue",
]

DEAL_PROPS = [
    "dealname", "dealstage", "amount", "closedate", "pipeline",
    "hubspot_owner_id", "closed_lost_reason", "closed_won_reason",
    "hs_closed_amount", "description",
]


def hs_get(path, params=None):
    r = requests.get(f"{HS_BASE}{path}", headers=HS_HEADERS, params=params or {})
    r.raise_for_status()
    return r.json()


def hs_post(path, body):
    r = requests.post(f"{HS_BASE}{path}", headers=HS_HEADERS, json=body)
    r.raise_for_status()
    return r.json()


def apollo_post(path, body):
    if not APOLLO_KEY:
        return {}
    r = requests.post(f"{AP_BASE}{path}", headers=AP_HEADERS, json=body)
    if r.status_code == 429:
        time.sleep(2)
        r = requests.post(f"{AP_BASE}{path}", headers=AP_HEADERS, json=body)
    r.raise_for_status()
    return r.json()


# ── Step 1: Build stage label→id map ────────────────────────────────

def get_stage_map():
    """Map deal stage labels to their IDs, and IDs to labels."""
    pipelines = hs_get("/crm/v3/pipelines/deals")
    label_to_id = {}
    id_to_label = {}
    for pipeline in pipelines.get("results", []):
        for stage in pipeline.get("stages", []):
            label = stage.get("label", "")
            sid = stage.get("stageId") or stage.get("id", "")
            label_to_id[label] = sid
            id_to_label[sid] = label
    return label_to_id, id_to_label


# ── Step 2: Search deals ────────────────────────────────────────────

def search_deals_by_stage_ids(stage_ids, limit=100, sort=None, sort_dir="DESCENDING"):
    """Search HubSpot deals by stage IDs, with company associations."""
    all_deals = []
    after = None
    while True:
        filters = [{"propertyName": "dealstage", "operator": "IN", "values": stage_ids}]
        body = {
            "filterGroups": [{"filters": filters}],
            "properties": DEAL_PROPS,
            "limit": min(limit - len(all_deals), 100),
        }
        if sort:
            body["sorts"] = [{"propertyName": sort, "direction": sort_dir}]
        if after:
            body["after"] = after
        data = hs_post("/crm/v3/objects/deals/search", body)
        results = data.get("results", [])
        all_deals.extend(results)
        paging = data.get("paging", {}).get("next", {})
        after = paging.get("after")
        if not after or len(all_deals) >= limit:
            break
    return all_deals[:limit]


def search_closed_lost(limit=50):
    """Search for closed-lost deals using hs_is_closed_won=false, sorted by close date."""
    all_deals = []
    after = None
    while True:
        body = {
            "filterGroups": [{"filters": [
                {"propertyName": "hs_is_closed_won", "operator": "EQ", "value": "false"},
                {"propertyName": "hs_is_closed", "operator": "EQ", "value": "true"},
            ]}],
            "properties": DEAL_PROPS,
            "sorts": [{"propertyName": "closedate", "direction": "DESCENDING"}],
            "limit": min(limit - len(all_deals), 100),
        }
        if after:
            body["after"] = after
        data = hs_post("/crm/v3/objects/deals/search", body)
        results = data.get("results", [])
        all_deals.extend(results)
        paging = data.get("paging", {}).get("next", {})
        after = paging.get("after")
        if not after or len(all_deals) >= limit:
            break
    return all_deals[:limit]


def get_company_ids_from_deals(deals):
    """Fetch company associations for deals via v3 per-deal endpoint."""
    deal_to_companies = {}
    all_company_ids = set()
    for d in deals:
        did = d["id"]
        try:
            data = hs_get(f"/crm/v3/objects/deals/{did}/associations/companies")
            cids = [r.get("id") for r in data.get("results", []) if r.get("id")]
            deal_to_companies[did] = cids
            all_company_ids.update(cids)
        except Exception:
            deal_to_companies[did] = []
    return deal_to_companies, all_company_ids


def get_companies_batch(company_ids):
    """Fetch company properties in batch."""
    companies = {}
    batch_size = 100
    for i in range(0, len(company_ids), batch_size):
        batch = list(company_ids)[i:i + batch_size]
        body = {
            "inputs": [{"id": cid} for cid in batch],
            "properties": COMPANY_PROPS,
        }
        data = hs_post("/crm/v3/objects/companies/batch/read", body)
        for result in data.get("results", []):
            companies[result["id"]] = result.get("properties", {})
    return companies


# ── Step 4: Search customers by lifecycle stage ─────────────────────

def search_customers():
    """Search for companies with lifecycle_stage = customer."""
    all_companies = []
    after = None
    while True:
        body = {
            "filterGroups": [{
                "filters": [{
                    "propertyName": "lifecyclestage",
                    "operator": "EQ",
                    "value": "customer",
                }]
            }],
            "properties": COMPANY_PROPS,
            "limit": 100,
        }
        if after:
            body["after"] = after
        data = hs_post("/crm/v3/objects/companies/search", body)
        all_companies.extend(data.get("results", []))
        paging = data.get("paging", {}).get("next", {})
        after = paging.get("after")
        if not after:
            break
    return all_companies


# ── Step 5: Apollo enrichment ───────────────────────────────────────

def enrich_via_apollo(domain):
    """Enrich a company via Apollo paid org enrichment (GET, ~1 credit)."""
    if not APOLLO_KEY or not domain:
        return {}
    try:
        r = requests.get(
            f"{AP_BASE}/organizations/enrich",
            headers=AP_HEADERS,
            params={"domain": domain},
        )
        if r.status_code == 429:
            time.sleep(2)
            r = requests.get(f"{AP_BASE}/organizations/enrich", headers=AP_HEADERS, params={"domain": domain})
        r.raise_for_status()
        data = r.json()
        org = data.get("organization", {})
        if org:
            return {
                "apollo_id": org.get("id"),
                "employees": org.get("estimated_num_employees"),
                "industry": org.get("industry"),
                "short_description": org.get("short_description"),
                "founded_year": org.get("founded_year"),
                "technologies": org.get("technology_names") or [],
                "keywords": org.get("keywords") or [],
                "total_funding": org.get("total_funding"),
                "latest_funding_stage": org.get("latest_funding_stage"),
                "annual_revenue": org.get("annual_revenue"),
                "revenue_printed": org.get("annual_revenue_printed"),
                "linkedin_url": org.get("linkedin_url"),
                "website_url": org.get("website_url"),
                "city": org.get("city"),
                "state": org.get("state"),
                "country": org.get("country"),
                "publicly_traded": org.get("publicly_traded_symbol"),
                "market_cap": org.get("market_cap"),
                "headcount_growth_6m": org.get("short_term_employee_growth_rate"),
                "languages": org.get("languages") or [],
                "suborganizations": [s.get("name") for s in (org.get("suborganizations") or [])[:5]],
            }
    except Exception as e:
        print(f"  Apollo enrichment failed for {domain}: {e}", file=sys.stderr)
    return {}


# ── Main ────────────────────────────────────────────────────────────

def main():
    print("Building stage map...")
    label_to_id, id_to_label = get_stage_map()
    print(f"  Found {len(label_to_id)} stages: {list(label_to_id.keys())}")

    late_stage_ids = [
        sid for label, sid in label_to_id.items()
        if any(label.lower().startswith(p) for p in LATE_STAGE_PREFIXES)
    ]
    closed_lost_ids = [
        sid for label, sid in label_to_id.items()
        if "lost" in label.lower()
    ]

    print(f"  Late-stage IDs: {late_stage_ids}")
    print(f"  Closed-lost IDs ({len(closed_lost_ids)}): {closed_lost_ids}")

    print("\nFetching customers...")
    customer_results = search_customers()
    print(f"  Found {len(customer_results)} customers")

    customers = []
    for c in customer_results:
        props = c.get("properties", {})
        domain = props.get("domain", "")
        entry = {
            "hubspot_id": c["id"],
            "name": props.get("name", ""),
            "domain": domain,
            "industry": props.get("industry"),
            "employees": props.get("numberofemployees"),
            "annual_revenue": props.get("annualrevenue"),
            "city": props.get("city"),
            "state": props.get("state"),
            "country": props.get("country"),
            "description": props.get("description"),
        }
        if domain:
            print(f"  Enriching {domain} via Apollo...")
            entry["apollo"] = enrich_via_apollo(domain)
            time.sleep(0.3)
        customers.append(entry)

    print("\nFetching late-stage deals...")
    if late_stage_ids:
        late_deals_raw = search_deals_by_stage_ids(late_stage_ids, limit=200)
    else:
        late_deals_raw = []
    print(f"  Found {len(late_deals_raw)} late-stage deals")

    deal_companies, all_company_ids = get_company_ids_from_deals(late_deals_raw)
    companies_data = get_companies_batch(all_company_ids) if all_company_ids else {}

    late_stage_deals = []
    seen_domains = set()
    for d in late_deals_raw:
        props = d.get("properties", {})
        stage_id = props.get("dealstage", "")
        stage_label = id_to_label.get(stage_id, stage_id)

        company_ids = deal_companies.get(d["id"], [])
        company_props = companies_data.get(company_ids[0], {}) if company_ids else {}
        domain = company_props.get("domain", "")

        entry = {
            "deal_id": d["id"],
            "deal_name": props.get("dealname", ""),
            "stage": stage_label,
            "amount": props.get("amount"),
            "close_date": props.get("closedate"),
            "company_name": company_props.get("name", ""),
            "domain": domain,
            "industry": company_props.get("industry"),
            "employees": company_props.get("numberofemployees"),
        }
        if domain and domain not in seen_domains:
            print(f"  Enriching {domain} via Apollo...")
            entry["apollo"] = enrich_via_apollo(domain)
            seen_domains.add(domain)
            time.sleep(0.3)
        late_stage_deals.append(entry)

    print("\nFetching closed-lost deals...")
    closed_lost_raw = search_closed_lost(limit=50)
    print(f"  Found {len(closed_lost_raw)} closed-lost deals")

    cl_deal_companies, cl_company_ids = get_company_ids_from_deals(closed_lost_raw)
    cl_companies_data = get_companies_batch(cl_company_ids) if cl_company_ids else {}

    closed_lost_deals = []
    for d in closed_lost_raw:
        props = d.get("properties", {})
        stage_id = props.get("dealstage", "")
        stage_label = id_to_label.get(stage_id, stage_id)

        company_ids = cl_deal_companies.get(d["id"], [])
        company_props = cl_companies_data.get(company_ids[0], {}) if company_ids else {}

        closed_lost_deals.append({
            "deal_id": d["id"],
            "deal_name": props.get("dealname", ""),
            "stage": stage_label,
            "amount": props.get("amount"),
            "close_date": props.get("closedate"),
            "loss_reason": props.get("closed_lost_reason"),
            "company_name": company_props.get("name", ""),
            "domain": company_props.get("domain", ""),
            "industry": company_props.get("industry"),
            "employees": company_props.get("numberofemployees"),
        })

    output = {
        "generated": time.strftime("%Y-%m-%d"),
        "stage_map": label_to_id,
        "customers": customers,
        "late_stage_deals": late_stage_deals,
        "closed_lost_deals": closed_lost_deals,
    }

    scripts_dir = Path(__file__).parent
    json_path = scripts_dir / "icp-data.json"
    json_path.write_text(json.dumps(output, indent=2, default=str))

    csv_fields = [
        "source", "name", "domain", "deal_stage", "deal_amount",
        "industry", "employees", "annual_revenue", "revenue_printed",
        "city", "state", "country", "founded_year",
        "publicly_traded", "market_cap", "headcount_growth_6m",
        "apollo_technologies", "apollo_keywords", "apollo_funding", "apollo_funding_stage",
        "short_description", "loss_reason",
    ]

    csv_path = scripts_dir / "icp-data.csv"
    with open(csv_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=csv_fields)
        writer.writeheader()

        def _row(source, name, domain, stage, amount, industry, employees,
                 ap=None, loss_reason=""):
            ap = ap or {}
            return {
                "source": source,
                "name": name,
                "domain": domain,
                "deal_stage": stage,
                "deal_amount": amount or "",
                "industry": industry or "",
                "employees": employees or "",
                "annual_revenue": ap.get("annual_revenue") or "",
                "revenue_printed": ap.get("revenue_printed") or "",
                "city": ap.get("city") or "",
                "state": ap.get("state") or "",
                "country": ap.get("country") or "",
                "founded_year": ap.get("founded_year") or "",
                "publicly_traded": ap.get("publicly_traded") or "",
                "market_cap": ap.get("market_cap") or "",
                "headcount_growth_6m": ap.get("headcount_growth_6m") or "",
                "apollo_technologies": "; ".join(ap.get("technologies", [])[:15]),
                "apollo_keywords": "; ".join(ap.get("keywords", [])[:10]),
                "apollo_funding": ap.get("total_funding") or "",
                "apollo_funding_stage": ap.get("latest_funding_stage") or "",
                "short_description": ap.get("short_description") or "",
                "loss_reason": loss_reason or "",
            }

        for c in customers:
            writer.writerow(_row(
                "customer", c.get("name", ""), c.get("domain", ""),
                "Closed Won", c.get("annual_revenue"),
                c.get("industry"), c.get("employees"),
                ap=c.get("apollo"),
            ))

        for d in late_stage_deals:
            writer.writerow(_row(
                "late_stage", d.get("company_name") or d.get("deal_name", ""),
                d.get("domain", ""), d.get("stage", ""), d.get("amount"),
                d.get("industry"), d.get("employees"),
                ap=d.get("apollo"),
            ))

        for d in closed_lost_deals:
            writer.writerow(_row(
                "closed_lost", d.get("company_name") or d.get("deal_name", ""),
                d.get("domain", ""), d.get("stage", ""), d.get("amount"),
                d.get("industry"), d.get("employees"),
                loss_reason=d.get("loss_reason"),
            ))

    prompt_path = scripts_dir / "icp-prompt.md"
    prompt_path.write_text(f"""# ICP Analysis Prompt

You have a CSV at `scripts/icp-data.csv` with {len(customers)} customers, {len(late_stage_deals)} late-stage deals, and {len(closed_lost_deals)} closed-lost deals. Each row has company firmographics from HubSpot + Apollo enrichment.

## Your task

Analyze this data and update the kazam YAML page at the path specified in the refresh block with a data-driven ICP. Structure your analysis as:

### 1. Customer DNA (P0 — highest confidence)
From the {len(customers)} existing customers, identify:
- **Company size sweet spot**: employee count range and median
- **Industry clusters**: which industries appear most
- **Geography**: where customers are concentrated
- **Tech stack signals**: common technologies (from Apollo)
- **Revenue/funding profile**: typical company stage and size
- What makes these companies similar? What's the archetype?

### 2. Pipeline validation (P1 — late-stage deals)
From the {len(late_stage_deals)} late-stage deals (Business Validation, Negotiation, Closed Won):
- Do they match the customer DNA or diverge?
- Any new segments emerging in the pipeline?
- Deal size patterns

### 3. Closed-lost patterns (P2 — lighter review)
From the {len(closed_lost_deals)} closed-lost deals:
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
""")

    print(f"\nWrote:")
    print(f"  {json_path} (full data)")
    print(f"  {csv_path} ({sum(1 for _ in open(csv_path)) - 1} rows)")
    print(f"  {prompt_path} (analysis prompt)")
    print(f"\nNext: have an agent read icp-prompt.md + icp-data.csv and update the ICP page.")


if __name__ == "__main__":
    main()
