# kazam - site build stats

Running ledger of sites built with kazam. LOC measures hand-authored source only (yaml + config), not the kazam binary or generated HTML.

## tylerdiderich.com

Rebuild of a Create-React-App personal site as kazam yaml.

| metric | before (CRA) | after (kazam) | Δ |
|---|---|---|---|
| source LOC | 1,722 | 210 | −88% |
| deps | react, react-router, axios, tailwind, react-gravatar, react-toggle, + 1000s transitive | 0 | - |
| build artifacts | `/build` (webpack) | `/_site` (static HTML) | - |
| deploy target | Firebase Hosting + 2 cloud functions | Firebase Hosting (static only) | - |

LOC breakdown (after):
- `kazam.yaml` - 14
- `index.yaml` - 70
- `resume.yaml` - 62
- `projects.yaml` - 32
- `blogs.yaml` - 21
- `firebase.json` + `.firebaserc` - 11

Baseline source counted: `src/**/*.{tsx,ts,css,js}` + `public/*.{html}`, excluding tests, `react-app-env.d.ts`, `setupTests`, `reportWebVitals`.

### Tokens
One-shot rebuild conversation (2026-04-18). `/cost` to be captured at end of session - track going forward to build a trend line across future sites.
