# Application language

The desktop renderer supports English and Spanish. Settings > Language offers
system language, Español and English. Spanish macOS UI languages resolve to
Spanish; every other system language resolves to English. The native macOS
CoreFoundation `CFLocaleCopyPreferredLanguages` via `sys-locale` result is authoritative after initialization.

The choice is stored separately in SQLite's `app_language_preference` config key
and in the renderer's `flowsight_language_preference` local storage key. A saved
native choice takes precedence on restart. A legacy renderer choice is migrated
only when no native language preference has been saved. If storage fails, the
selected language still applies to the current session and Settings explains
that persistence failed.

## Text and state boundaries

The catalogue translates source-owned UI text and explicit native statuses.
Literal markup is translated before interpolating values; runtime task titles,
names, descriptions, messages, IDs, OAuth values and paths never become
translation keys. Canonical activity categories have translated display labels
while their stored keys remain unchanged. No navigation reload is needed.

Language switches refresh text bindings and presentation-only summary/report
surfaces. Tracking state, elapsed time, editable inputs and pending plan IDs are
preserved. The planner carries bilingual host summary, overflow and rest
metadata; a user task named `Break` or `Review` remains its original name.
Estimates originating from assumptions remain explicitly labelled.

Reports and their PDF exports select `localized_report.en` or
`localized_report.es` from the same evidence and grounded selection. Switching
languages does not request another model run. Tray labels and local reminders
use the saved language. Reminder evidence, permission and cooldown checks remain
in the native host.

## Verification

- `pnpm --filter @flowsight/agent test:renderer` checks the catalogue's template
  parameters, data preservation and Spanish screen/PDF output, as well as
  tracking, updater and installation repair behavior.
- `node scripts/verify-localization.mjs` uses the production renderer with
  fictional native responses. Set `FLOWSIGHT_RENDERER_URL` to a running Vite
  preview. It covers system and manual preferences, differing macOS/browser
  languages, 340/370/900 pixel widths, rest-label containment, active tracking,
  planner inputs/drafts, persistence, storage errors and absence of calendar or
  tracking mutations during a switch. Captures are synthetic UI evidence.
- Native unit tests cover preference resolution and isolated SQLite persistence
  without changing another config key, Spanish reminder copy with original
  task/app names, and bilingual planner metadata.

The PDF's built-in font supports Spanish Latin characters. Existing handling
for app names that require another font gives an explicit on-screen-report
reference. The renderer preserves those original names.
