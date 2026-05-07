#!/usr/bin/env python3
"""
Read kazam freshness notify JSON and send Slack DMs to each content owner.

Maps owner email addresses to Slack user IDs, then sends each owner a
summary of their stale pages with action links.

Env vars: SLACK_BOT_TOKEN
Input: kazam freshness notify --json (piped via stdin or --file)

Usage:
  kazam freshness notify --json | python3 scripts/freshness-notifier.py
  python3 scripts/freshness-notifier.py --file scripts/freshness-digest.json
  python3 scripts/freshness-notifier.py --dry-run  # preview messages without sending
"""

# ── Customization block ──────────────────────────────────────────────────────
#
# SITE_BASE_URL: Base URL for page links in Slack messages. Set to your
#   deployed site URL so owners can click through to the actual page.
#   Leave empty to omit links.
#
# OWNER_EMAIL_DOMAIN: If your freshness owners use bare usernames (e.g. "tyler")
#   instead of full emails, append this domain to look up Slack users.
#   Leave empty if owners are already full email addresses.
#
# MESSAGE_HEADER: Customize the greeting at the top of each DM.

SITE_BASE_URL = ""
OWNER_EMAIL_DOMAIN = ""
MESSAGE_HEADER = "Hey! Some docs you own need attention:"

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

SLACK_TOKEN = os.environ.get("SLACK_BOT_TOKEN", "")
if not SLACK_TOKEN:
    sys.exit("SLACK_BOT_TOKEN not set — add it to .env")

SLACK_HEADERS = {
    "Authorization": f"Bearer {SLACK_TOKEN}",
    "Content-Type": "application/json; charset=utf-8",
}


def slack_get(path, params=None):
    r = requests.get(f"https://slack.com/api/{path}", headers=SLACK_HEADERS, params=params or {})
    r.raise_for_status()
    data = r.json()
    if not data.get("ok"):
        print(f"  Slack API error ({path}): {data.get('error')}", file=sys.stderr)
    return data


def slack_post(path, body):
    r = requests.post(f"https://slack.com/api/{path}", headers=SLACK_HEADERS, json=body)
    r.raise_for_status()
    data = r.json()
    if not data.get("ok"):
        print(f"  Slack API error ({path}): {data.get('error')}", file=sys.stderr)
    return data


def resolve_email(email):
    """Resolve an email to a Slack user ID."""
    if OWNER_EMAIL_DOMAIN and "@" not in email:
        email = f"{email}@{OWNER_EMAIL_DOMAIN}"
    data = slack_get("users.lookupByEmail", {"email": email})
    if data.get("ok"):
        return data["user"]["id"]
    return None


def format_message(owner, pages):
    """Format a Slack message for one owner."""
    lines = [MESSAGE_HEADER, ""]

    for page in pages:
        status = page["status"].upper().replace("_", " ")
        days = page["days"]
        title = page["title"]
        path = page["path"]

        if status == "EXPIRED":
            emoji = ":no_entry:"
            label = f"EXPIRED {days}d ago"
        elif status == "OVERDUE":
            emoji = ":warning:"
            label = f"OVERDUE {days}d"
        else:
            emoji = ":hourglass_flowing_sand:"
            label = f"due in {days}d"

        if SITE_BASE_URL:
            page_url = path.replace(".yaml", ".html")
            line = f"{emoji} *[{label}]* <{SITE_BASE_URL}/{page_url}|{title}>"
        else:
            line = f"{emoji} *[{label}]* {title} (`{path}`)"

        lines.append(f"  {line}")

    lines.append("")
    lines.append("To mark a page as reviewed: `kazam freshness act <path> refresh`")
    lines.append("To archive: `kazam freshness act <path> archive`")

    return "\n".join(lines)


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Send Slack DMs for stale kazam pages")
    parser.add_argument("--file", help="Read JSON from file instead of stdin")
    parser.add_argument("--dry-run", action="store_true", help="Preview messages without sending")
    parser.add_argument("--channel", help="Post to a channel instead of DMs (channel ID or name)")
    args = parser.parse_args()

    if args.file:
        data = json.loads(Path(args.file).read_text())
    else:
        if sys.stdin.isatty():
            print("Reading from stdin — pipe kazam freshness notify --json", file=sys.stderr)
        data = json.load(sys.stdin)

    owners = data.get("owners", [])
    if not owners:
        print("No stale pages to notify about.")
        return

    sent = 0
    skipped = 0

    for owner_data in owners:
        owner = owner_data["owner"]
        pages = owner_data["pages"]

        if owner == "(unowned)":
            if args.channel:
                pass  # will post to channel below
            else:
                print(f"  Skipping {len(pages)} unowned pages (no DM target)")
                skipped += len(pages)
                continue

        message = format_message(owner, pages)

        if args.dry_run:
            print(f"\n{'='*60}")
            print(f"TO: {owner} ({len(pages)} pages)")
            print(f"{'='*60}")
            print(message)
            sent += 1
            continue

        if args.channel:
            slack_post("chat.postMessage", {
                "channel": args.channel,
                "text": f"*{owner}* — {len(pages)} page(s) need review:\n{message}",
            })
            sent += 1
            time.sleep(0.5)
        else:
            user_id = resolve_email(owner)
            if not user_id:
                print(f"  Could not find Slack user for {owner} — skipping", file=sys.stderr)
                skipped += 1
                continue

            dm = slack_post("conversations.open", {"users": user_id})
            if not dm.get("ok"):
                print(f"  Could not open DM with {owner} — skipping", file=sys.stderr)
                skipped += 1
                continue

            channel_id = dm["channel"]["id"]
            slack_post("chat.postMessage", {
                "channel": channel_id,
                "text": message,
            })
            sent += 1
            time.sleep(0.5)

    print(f"\nDone: {sent} messages sent, {skipped} skipped")


if __name__ == "__main__":
    main()
