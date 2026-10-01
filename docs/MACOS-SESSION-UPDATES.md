# macOS session and setup updates

This change extends the existing 5.0.3 macOS implementation. It preserves the
platform capture loop, Accessibility integration, Metal runtime, authentication,
report renderer, and native window configuration.

## Implemented behavior

- A local session planner uses the reviewed Qwen task estimator. The host
  validates all requested tasks and computes availability, fixed commitments,
  and breaks. Drafting and revision do not write calendar events. Explicit
  confirmation saves work and breaks to the linked Google or Microsoft default
  calendar, or locally when none is connected. Availability is checked before
  planning and saving. An encrypted owner-scoped journal supports explicit retry
  or stopping remaining writes while preserving already confirmed events.
- Five optional setup steps cover work preferences, planning, reminder consent,
  weekly PDF reports and eligible cloud-calendar connections.
- English and Spanish follow the native system language by default, with a
  persistent language choice in Settings. Counted exercises produce separate
  work blocks with real rests and visibly provisional missing estimates.
- Pro Coach uses a pure escaped Markdown renderer for saved and new replies.
  Canonical shared cloud sources accompany the native contract tests. No cloud
  deployment is performed by this platform PR. Notion remains disabled.
- The Break label stays inside its illustrated block. The notification figure
  uses shipped reminder wording and labels its contents as an example.
- Generic reminder consent and contextual reminder consent remain separate.
  Displaying the example has no native notification or tracking side effects.
  Actual reminder delivery uses the macOS notification plugin during tracking.
- Planner state is authenticated AES-256-GCM ciphertext in SQLite. Its random
  key is held in Keychain. Read operations never create a missing key; creation
  is serialized. A native test uses an exclusive temporary Keychain service.
- Existing report databases gain optional application and explicit selected
  task columns for reminder classification. Raw window titles stay absent.
- Weekly reports use the existing native local report generator and PDF
  renderer, a real folder chooser, and the guarded PDF writer. Failed runs have
  a five-minute retry delay. The native pulse and renderer polling both use a
  shared busy guard.
- The dark stylesheet is connected to the theme controller and keeps subdued
  hover fills with keyboard focus outlines.

## Verification

The PR runs on `macos-14`: native Metal runtime preparation and executable smoke
audit, renderer unit tests, Vite production build, Rust formatting, Cargo check,
Rust unit tests (including the isolated Keychain test), Clippy with warnings
denied, `tauri build --no-bundle`, and native MCP STDIO execution.

Browser integration checks use the actual renderer with isolated synthetic
native responses. They cover optional setup at 340×400, 370×700, and
900×800, the SVG label bounds, example/contextual notifications, error paths,
independent consent, revision without writes, and exactly one explicit
calendar confirmation, saved/new Coach replies, language switching and linked
calendar restart/retry/abandonment. The ADDA replay uses the measured Qwen fixture: four
topics, seven blocks, three rests, and revision to PLE first with fifteen-minute
breaks. Screenshots are uploaded as labelled renderer previews. These checks
do not claim to simulate macOS system notification presentation or a physical
Mac user's Accessibility permissions.

The PR must remain a draft until native verification on its final commit passes.

The native Qwen probe extracts the actual planner core and request builder,
records their SHA-256 hashes, and uses only synthetic context. It requires
tool calls for the initial ADDA request and PLE-first revision, with all four
topics, three correctly timed breaks, valid bounds, and no calendar writes.
Both the production runtime and smoke checks allocate 8192 context tokens
across two inference slots, avoiding the old 2048-token per-slot limit.
