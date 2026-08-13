# DESC: Medium Python repo (233 files) - cross-cutting change across multiple modules
# Tests a change that touches several files - the agent needs to find all the right places.

REPO="$HOME/maze-repos/technical-success-hub"
MODEL="claude-sonnet-4-6"

PROMPT='Add a "last_synced" timestamp field to every customer data model in the codebase. Find all customer-related data classes/models, add the field (optional, defaults to None), and update any serialization/deserialization logic that needs to handle the new field. Make the actual code changes.'
