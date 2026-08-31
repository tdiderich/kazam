# DESC: Mid-size Python repo (8k files) - add a CLI flag to an existing tool
# Tests navigating a sprawling internal tools repo to find and modify the right file.

REPO="$HOME/maze-repos/Ops---Internal-Tooling"
MODEL="claude-sonnet-4-6"

PROMPT='In the Customer Reporting tool, add a --days flag (default 30) that limits the reporting window. Find the CLI entry point, add the argparse flag, and thread it through to the query layer so it filters results to the last N days. Make the actual code changes.'
