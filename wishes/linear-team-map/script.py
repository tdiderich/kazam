#!/usr/bin/env python3
"""
Pull team/people/project ownership data from Linear and write JSON + CSV + analysis prompt.

Uses Linear's GraphQL API to:
  1. Fetch all teams
  2. Fetch all users (active only)
  3. Fetch recent issues (last 30 days) to infer team membership from assignments
  4. Fetch active projects and map members

Env vars: LINEAR_API_KEY
Output:
  scripts/team-map-data.json   - full structured data
  scripts/team-map-data.csv    - flat CSV for agent review
  scripts/team-map-prompt.md   - analysis prompt for the agent
"""

# ── Customization block ──────────────────────────────────────────────────────
#
# LOOKBACK_DAYS: How many days of issue history to scan for team membership.
#   Increase for orgs with less frequent issue activity.
#
# EXCLUDED_TEAMS: Team names (case-insensitive) to exclude from the report.
#   Useful for test teams, archived teams, or bot-only teams.
#
# EXCLUDED_USERS: Email addresses to exclude (bots, service accounts).

LOOKBACK_DAYS = 30
EXCLUDED_TEAMS = []
EXCLUDED_USERS = []

# ────────────────────────────────────────────────────────────────────────────

import csv
import json
import os
import sys
import time
from collections import defaultdict
from datetime import datetime, timedelta
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

LINEAR_KEY = os.environ.get("LINEAR_API_KEY", "")
if not LINEAR_KEY:
    sys.exit("LINEAR_API_KEY not set - add it to .env")

LINEAR_URL = "https://api.linear.app/graphql"
HEADERS = {"Authorization": LINEAR_KEY, "Content-Type": "application/json"}


def gql(query, variables=None):
    body = {"query": query}
    if variables:
        body["variables"] = variables
    r = requests.post(LINEAR_URL, headers=HEADERS, json=body)
    if r.status_code == 429:
        time.sleep(2)
        r = requests.post(LINEAR_URL, headers=HEADERS, json=body)
    r.raise_for_status()
    data = r.json()
    if "errors" in data:
        print(f"  GraphQL errors: {data['errors']}", file=sys.stderr)
    return data.get("data", {})


# ── Step 1: Fetch teams ───────────────────────────────────────────

def fetch_teams():
    query = """
    query {
      teams(first: 100) {
        nodes {
          id
          name
          key
          description
        }
      }
    }
    """
    data = gql(query)
    teams = data.get("teams", {}).get("nodes", [])
    excluded = {t.lower() for t in EXCLUDED_TEAMS}
    return [t for t in teams if t["name"].lower() not in excluded]


# ── Step 2: Fetch users ──────────────────────────────────────────

def fetch_users():
    query = """
    query {
      users(first: 250, filter: { active: { eq: true } }) {
        nodes {
          id
          name
          email
          displayName
          admin
        }
      }
    }
    """
    data = gql(query)
    users = data.get("users", {}).get("nodes", [])
    excluded = {e.lower() for e in EXCLUDED_USERS}
    return [u for u in users if u.get("email", "").lower() not in excluded]


# ── Step 3: Fetch recent issues to infer team membership ─────────

def fetch_recent_issues(team_id, since_iso):
    all_issues = []
    cursor = None
    while True:
        after_clause = f', after: "{cursor}"' if cursor else ""
        query = f"""
        query {{
          team(id: "{team_id}") {{
            issues(
              first: 100,
              filter: {{ updatedAt: {{ gte: "{since_iso}" }} }}
              {after_clause}
            ) {{
              nodes {{
                id
                title
                assignee {{ id name }}
                project {{ id name }}
                state {{ name type }}
                priority
                updatedAt
              }}
              pageInfo {{ hasNextPage endCursor }}
            }}
          }}
        }}
        """
        data = gql(query)
        team_data = data.get("team", {})
        issues_data = team_data.get("issues", {})
        nodes = issues_data.get("nodes", [])
        all_issues.extend(nodes)
        page_info = issues_data.get("pageInfo", {})
        if page_info.get("hasNextPage") and page_info.get("endCursor"):
            cursor = page_info["endCursor"]
        else:
            break
    return all_issues


# ── Step 4: Fetch active projects ────────────────────────────────

def fetch_projects():
    all_projects = []
    cursor = None
    while True:
        after_clause = f', after: "{cursor}"' if cursor else ""
        query = f"""
        query {{
          projects(
            first: 50,
            filter: {{ state: {{ type: {{ in: ["started", "planned"] }} }} }}
            {after_clause}
          ) {{
            nodes {{
              id
              name
              state {{ name type }}
              teams {{ nodes {{ id name }} }}
              members {{ nodes {{ id name }} }}
              lead {{ id name }}
              targetDate
              progress
            }}
            pageInfo {{ hasNextPage endCursor }}
          }}
        }}
        """
        data = gql(query)
        projects_data = data.get("projects", {})
        nodes = projects_data.get("nodes", [])
        all_projects.extend(nodes)
        page_info = projects_data.get("pageInfo", {})
        if page_info.get("hasNextPage") and page_info.get("endCursor"):
            cursor = page_info["endCursor"]
        else:
            break
    return all_projects


# ── Main ────────────────────────────────────────────────────────────

def main():
    since = (datetime.utcnow() - timedelta(days=LOOKBACK_DAYS)).strftime("%Y-%m-%dT00:00:00.000Z")

    print("Fetching teams...")
    teams = fetch_teams()
    print(f"  Found {len(teams)} teams")
    teams_by_id = {t["id"]: t for t in teams}

    print("Fetching users...")
    users = fetch_users()
    print(f"  Found {len(users)} active users")
    users_by_id = {u["id"]: u for u in users}

    print("Fetching active projects...")
    projects = fetch_projects()
    print(f"  Found {len(projects)} active projects")

    # Build team membership from issue assignments
    team_members = defaultdict(lambda: defaultdict(int))  # team_id -> user_id -> issue_count
    team_projects = defaultdict(set)  # team_id -> set of project names

    for team in teams:
        tid = team["id"]
        print(f"  Scanning issues for {team['name']}...")
        issues = fetch_recent_issues(tid, since)
        print(f"    {len(issues)} issues in last {LOOKBACK_DAYS} days")

        for issue in issues:
            assignee = issue.get("assignee")
            if assignee and assignee.get("id"):
                team_members[tid][assignee["id"]] += 1
            project = issue.get("project")
            if project and project.get("name"):
                team_projects[tid].add(project["name"])

    # Build output structure
    team_output = []
    for team in teams:
        tid = team["id"]
        members = []
        for uid, count in sorted(team_members[tid].items(), key=lambda x: -x[1]):
            user = users_by_id.get(uid, {})
            members.append({
                "name": user.get("name", "Unknown"),
                "email": user.get("email", ""),
                "issue_count": count,
            })
        team_output.append({
            "id": tid,
            "name": team["name"],
            "key": team.get("key", ""),
            "description": team.get("description", ""),
            "members": members,
            "active_projects": sorted(team_projects[tid]),
        })

    project_output = []
    for p in projects:
        lead = p.get("lead")
        project_teams = [t["name"] for t in (p.get("teams", {}).get("nodes", []))]
        project_members = [m["name"] for m in (p.get("members", {}).get("nodes", []))]
        project_output.append({
            "name": p["name"],
            "state": p.get("state", {}).get("name", ""),
            "teams": project_teams,
            "lead": lead.get("name") if lead else None,
            "members": project_members,
            "target_date": p.get("targetDate"),
            "progress": p.get("progress"),
        })

    # Cross-team people (appear on 2+ teams)
    user_teams = defaultdict(list)
    for team in team_output:
        for member in team["members"]:
            user_teams[member["name"]].append(team["name"])
    cross_team = {name: teams for name, teams in user_teams.items() if len(teams) >= 2}

    # People with no recent activity
    active_user_ids = set()
    for members in team_members.values():
        active_user_ids.update(members.keys())
    inactive = [u for u in users if u["id"] not in active_user_ids]

    output = {
        "generated": time.strftime("%Y-%m-%d"),
        "lookback_days": LOOKBACK_DAYS,
        "teams": team_output,
        "projects": project_output,
        "cross_team_people": cross_team,
        "inactive_users": [{"name": u["name"], "email": u.get("email", "")} for u in inactive],
    }

    scripts_dir = Path(__file__).parent
    json_path = scripts_dir / "team-map-data.json"
    json_path.write_text(json.dumps(output, indent=2, default=str))

    # Write flat CSV
    csv_path = scripts_dir / "team-map-data.csv"
    csv_fields = [
        "team", "person", "email", "issue_count_30d", "active_projects",
    ]
    with open(csv_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=csv_fields)
        writer.writeheader()
        for team in team_output:
            for member in team["members"]:
                writer.writerow({
                    "team": team["name"],
                    "person": member["name"],
                    "email": member["email"],
                    "issue_count_30d": member["issue_count"],
                    "active_projects": "; ".join(team["active_projects"]),
                })

    total_rows = sum(len(t["members"]) for t in team_output)

    prompt_path = scripts_dir / "team-map-prompt.md"
    prompt_path.write_text(f"""# Team Map Analysis Prompt

You have structured data from Linear at `scripts/team-map-data.json` and a flat CSV at `scripts/team-map-data.csv` with {total_rows} team-person rows across {len(team_output)} teams and {len(project_output)} active projects.

## Your task

Analyze the data and update the kazam YAML page at the path specified in the refresh block. Structure as:

### Per-team sections
For each team with active members, create a section with:
- Team description (from Linear or inferred from project names)
- **Team lead** if identifiable (highest issue count or project lead)
- Table of members: Person | Focus area (inferred from projects and issue volume)
- Active projects listed

### Cross-functional people
List people who appear on 2+ teams: {json.dumps(list(cross_team.keys()))}

### Key observations
Flag:
- **Single points of failure**: Teams with only 1 active member
- **Inactive users**: {len(inactive)} users with no Linear activity in {LOOKBACK_DAYS} days (may use other tools or have non-engineering roles)
- **Overloaded people**: Anyone on 3+ teams

Keep the page structure as kazam YAML with `type: section` and `type: markdown` components. Use tables for team rosters.
""")

    print(f"\nWrote:")
    print(f"  {json_path} (full data)")
    print(f"  {csv_path} ({total_rows} rows)")
    print(f"  {prompt_path} (analysis prompt)")
    print(f"\nNext: have an agent read team-map-prompt.md + team-map-data.json and update the page.")


if __name__ == "__main__":
    main()
