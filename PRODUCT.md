# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The primary interface is a compact Tauri desktop WebView packaged for macOS. Its design follows desktop operating conventions even though the renderer is HTML/CSS.

## Users

Individuals doing knowledge work, especially developers, who want to understand their own work patterns without turning activity data into employee surveillance.

## Product Purpose

FlowSight records and interprets local work activity so a person can review tracked time, work categories, interruptions, and context, then decide what to change. A useful result is an evidence-grounded review, not a productivity score.

## Positioning

Sensitive screen-context analysis runs locally. The user chooses when tracking runs and separately opts into optional cloud features.

## Operating Context

- A narrow desktop window defaults to 370 × 700 px and can be resized down to 340 × 400 px.
- The core tasks are Today (tracking, goal and task), Insights (activity and a local work report), and Settings (account, preferences and integrations).
- The title bar uses native macOS traffic lights; the renderer reserves their space.
- Coach is a conversational cloud feature shown with an eligibility gate. It must not be presented as unlocked merely because the user signs in.

## Capabilities and Constraints

- Preserve working Tauri commands, element IDs, tracking controls, onboarding, license gates and report export while changing the visual presentation.
- The legacy activity summary derives focus-category time from categorized entries. Do not label it as sustained-focus blocks.
- The local work report may use AI or a rule-based fallback; UI copy must not promise AI when fallback is possible.
- Tracking and cloud access remain under the user's control. Do not imply a purchase or entitlement that has not been verified.
- The web renderer must remain readable in system light and dark appearance.

## Brand Commitments

- Retain the FlowSight name and existing logo.
- The approved desktop visual direction comes from the FlowSight mobile family: a light gridded canvas, slate typography, teal-led signals, softly layered panels and one prominent tracking action. Adapt its density for a resizable desktop window rather than copying a phone screen pixel for pixel.
- Ratio was an earlier reference, but the user rejected the Ratio-led desktop rendition. The mobile app is the governing visual authority.

## Evidence on Hand

The product behavior is documented by `README.md`, `apps/agent/src/renderer/index.html` and `apps/agent/src-tauri/tauri.conf.json`. The approved cross-platform visual authority is the FlowSight.AI v4.1 desktop renderer and its DESIGN.md. No independent usability or productivity benchmarks were supplied.

## Product Principles

1. Show what was observed and separate it from interpretation.
2. Keep tracking and sharing under the user's control.
3. Keep the primary tracking action usable at the minimum window size.
4. Remain useful when cloud features or local AI are unavailable.
