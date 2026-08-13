# DESC: Small React/TS app (89 files) - add a loading state to an async component
# Concrete change in a small repo - control case where kazam overhead should be minimal.

REPO="$HOME/personal-repos/curata"
MODEL="claude-sonnet-4-6"

PROMPT='In this React/TypeScript app, find the main data-fetching component (the one that loads newsletter or content data from an API or data source). Add a loading skeleton state to it:

1. Find the component that fetches data on mount or via a hook
2. Add a loading state that shows a simple skeleton/placeholder while data is being fetched (use a basic div with a "loading" class and pulsing opacity - do not install any new packages)
3. Add the CSS for the skeleton animation in the appropriate stylesheet
4. Make sure the loading state clears when data arrives and error state is handled

Make the actual code changes.'
