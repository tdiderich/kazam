# DESC: Plugin repo (126 files) - add a config field to an existing skill and thread it through
# Forces pattern discovery + concrete multi-file change across skill definition, execution, and tests.

REPO="$HOME/maze-repos/maze-claude-plugins"
MODEL="claude-sonnet-4-6"

PROMPT='In the maze-claude-plugins repo, pick any existing skill that has both a definition file and an execution/implementation file. Add a new optional configuration field called "timeout_seconds" (default: 300) to that skill:

1. Find how skills are defined - look at the structure of 2-3 existing skills to understand the pattern
2. Add the "timeout_seconds" field to the skill definition/config
3. Thread it through to the execution layer so it is available at runtime (even if it is just logged or stored - it does not need to actually enforce a timeout)
4. If the skill has tests, update them to cover the new field

Make the actual code changes.'
