---
name: FlowSight Mac Desktop Renderer
description: The FlowSight mobile visual language adapted to a compact macOS work window.
colors:
  canvas-light: "#fbfcfb"
  canvas-dark: "#101d25"
  surface-white: "#ffffff"
  card-light: "rgb(255 255 255 / 0.97)"
  card-dark: "#1a2c36"
  ink-light: "#303c50"
  ink-dark: "#e5f0f1"
  muted-light: "#586c7d"
  muted-dark: "#b4c5cf"
  border-light: "rgb(227 236 240 / 0.9)"
  border-dark: "#2a414b"
  grid-light: "rgb(63 93 120 / 0.075)"
  grid-dark: "rgb(194 226 233 / 0.055)"
  action-teal: "#087f78"
  action-teal-hover: "#076f69"
  signal-teal-dark: "#75ddd0"
  progress-teal: "#25b9ad"
  active-teal-light: "#e8f8f5"
  tracking-indigo: "#5d61c9"
  tracking-blue: "#2b7898"
  tracking-teal: "#0b7b75"
typography:
  display:
    fontFamily: "Manrope, sans-serif"
    fontSize: "46px"
    fontWeight: 800
    lineHeight: 1.12
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Manrope, sans-serif"
    fontSize: "34px"
    fontWeight: 800
    lineHeight: 1.16
    letterSpacing: "-0.04em"
  title:
    fontFamily: "Manrope, sans-serif"
    fontSize: "15px"
    fontWeight: 800
    letterSpacing: "-0.025em"
  body:
    fontFamily: "'Plus Jakarta Sans', sans-serif"
    fontSize: "13px"
    lineHeight: 1.5
  label:
    fontFamily: "'Plus Jakarta Sans', sans-serif"
    fontSize: "11px"
    fontWeight: 800
    lineHeight: 1.2
    letterSpacing: "0.16em"
rounded:
  timer: "27px"
  card: "20px"
  dock: "24px"
  action: "16px"
  control: "11px"
  dialog: "22px"
  pill: "999px"
spacing:
  tight: "8px"
  stack: "12px"
  surface: "20px"
  content-inline: "20px"
  content-block: "24px"
  wide-inline: "28px"
components:
  button-primary:
    backgroundColor: "{colors.action-teal}"
    textColor: "{colors.surface-white}"
    rounded: "{rounded.control}"
    padding: "8px 13px"
    height: "38px"
  button-primary-hover:
    backgroundColor: "{colors.action-teal-hover}"
    textColor: "{colors.surface-white}"
    rounded: "{rounded.control}"
  button-secondary:
    backgroundColor: "#f7fbfa"
    textColor: "#2b5961"
    rounded: "{rounded.control}"
    padding: "8px 13px"
    height: "38px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "#147d76"
    rounded: "{rounded.control}"
    padding: "8px 13px"
    height: "38px"
  button-tracking:
    textColor: "{colors.surface-white}"
    rounded: "{rounded.action}"
    padding: "0 16px"
    height: "52px"
  input:
    backgroundColor: "{colors.surface-white}"
    textColor: "#34445a"
    rounded: "{rounded.control}"
    padding: "8px 11px"
    height: "39px"
  card:
    backgroundColor: "{colors.card-light}"
    textColor: "{colors.ink-light}"
    rounded: "{rounded.card}"
    padding: "20px"
  badge-success:
    backgroundColor: "#def7f0"
    textColor: "#0a766c"
    rounded: "{rounded.pill}"
    padding: "5px 8px"
  bottom-nav:
    backgroundColor: "rgb(255 255 255 / 0.96)"
    textColor: "{colors.muted-light}"
    rounded: "{rounded.dock}"
    padding: "7px"
    width: "min(calc(100% - 24px), 540px)"
  timer-surface:
    textColor: "{colors.ink-light}"
    rounded: "{rounded.timer}"
    padding: "22px 22px 20px"
  consent-dialog:
    backgroundColor: "{colors.surface-white}"
    textColor: "{colors.ink-light}"
    rounded: "{rounded.dialog}"
    padding: "24px"
    width: "min(100%, 470px)"
---

# Design System: FlowSight Mac Desktop Renderer

## Overview

**Creative North Star: "The Gridded Work Companion"**

The macOS renderer belongs to the FlowSight mobile family. A pale technical grid, slate reading text, teal signals, softly lifted white panels, and one indigo-to-teal tracking action make a narrow work window feel calm and capable. The layout adapts the family's density to a resizable desktop surface rather than enlarging a phone screen.

Today puts the tracking decision beside measured time; Insights, Coach, Settings, first run, and consent use quieter cards and rails for supporting context. This is the visual contract for the desktop WebView. Native macOS traffic lights remain the window's own controls.

**Key Characteristics:**

- A 56px pale grid behind centered, softly layered work surfaces.
- Manrope headings and measured figures with Plus Jakarta Sans reading and controls.
- Teal for state and progress; the indigo-to-blue-to-teal gradient is concentrated in tracking and selected first-run actions.
- A floating four-tab dock whose Coach entry remains visible while access is gated.

## Colors

The light appearance is airy and low-contrast in its surfaces, with a complete deep-slate system-dark counterpart. The frontmatter contains the normative colors; component-specific tints remain in the implementation.

### Primary

- **Action Teal** (action-teal / action-teal-hover) is the solid affirmative color for routine controls, with the deeper value on hover. **Dark Signal Teal** (signal-teal-dark) keeps active labels readable on dark surfaces.
- **Progress Teal** (progress-teal) fills measured rails and week marks. **Soft Active Teal** (active-teal-light) marks a selected dock item without coloring the whole surface.

### Secondary

- **Tracking Gradient** (tracking-indigo / tracking-blue / tracking-teal) is a three-stop, 104-degree treatment for the main tracking action and selected first-run controls, not a general accent for cards.

### Neutral

- **Gridded Canvas** (canvas-light / canvas-dark) and **Grid Lines** (grid-light / grid-dark) establish the quiet 56px field.
- **Card Surface** (card-light / card-dark) and **Solid White** (surface-white) distinguish translucent light cards from opaque fields and dialogs.
- **Slate Reading Ink** (ink-light / ink-dark), **Supporting Text** (muted-light / muted-dark), and **Panel Borders** (border-light / border-dark) keep information legible without heavy dividers.

**The Signal Rule.** Teal denotes a real state, progress, selection, or action; reserve the gradient for tracking and selected first-run decisions.

## Typography

**Display Font:** Manrope, with a sans-serif fallback.

**Body Font:** Plus Jakarta Sans, with a sans-serif fallback.

Manrope gives headings and measured time a confident, friendly shape. Plus Jakarta Sans keeps explanations, forms, and compact settings readable in the narrow window.

### Hierarchy

- **Display** (800, 46px, 1.12): centered elapsed time, with tabular numerals; 40px at 380px and narrower, 52px at 600px and wider, and 36px in short windows.
- **Headline** (800, 34px, 1.16): Today, Insights, and Settings headings; 30px at 380px and narrower.
- **Title** (800, 15px): card headings and section titles.
- **Body** (13px, 1.5): standard reading and control text; supporting explanations often use 11–12px.
- **Kicker** (800, 11px, 0.16em, uppercase): concise section cues.

**The Measurement Rule.** Align measured figures with tabular numerals while retaining the same Manrope display voice.

## Layout

The default Tauri window is 370 × 700px and remains usable at 340 × 400px. An HTML title bar (48px after theme overrides) reserves 72px at its left for native macOS traffic lights. Below it, the scrollable pane uses 24px top and 20px side padding and reserves 112px after content so the dock does not cover controls. The background grid repeats every 56px.

Today centers within 640px; Insights, Settings, and Coach center within 760px. At 600px and wider, content padding becomes 30px vertically and 28px horizontally, Today goal and task cards can form two columns, and Settings gains two columns. At 380px and narrower, content padding contracts to 18px top and 15px sides. In windows 520px tall or shorter, Today compresses its heading, timer, and buttons while keeping the action reachable.

The dock floats 14px above the bottom, has 12px outer margins, and stops growing at 540px. It moves to 8px above the bottom in narrow widths and 7px in short windows. Consent is bounded by viewport height; the monitoring dialog keeps actions anchored while its details scroll.

## Elevation & Depth

Depth is soft and functional: fine card borders and diffuse shadows separate reading surfaces from the grid; the timer adds a faint cool tint. The floating dock and blocking overlays have stronger lift because they sit above scrolling content. Dark mode relies more on deep-slate tonal layers and borders than on bright shadow contrast.

### Shadow Vocabulary

- **Surface lift** (`0 8px 28px rgb(30 53 68 / 0.045)`): the shared low ambient shadow token.
- **Card lift** (`0 8px 26px rgb(44 76 94 / 0.045)`): ordinary light cards.
- **Dock lift** (`0 18px 36px rgb(40 67 86 / 0.13), 0 3px 9px rgb(40 67 86 / 0.06)`): persistent navigation above content.
- **Consent lift** (`0 20px 56px rgb(23 50 64 / 0.2)`): the blocking monitoring decision.

**The Layer Rule.** Give ordinary content a quiet lift; reserve the strongest elevation for persistent navigation and blocking overlays.

## Shapes

The timer has generous corners (27px), regular cards have soft corners (20px, sometimes 21px in compact Today cards), the dock uses 24px, and the monitoring dialog uses 22px. The tracking action uses 16px; ordinary fields and buttons use 11px. Badges, week marks, and measured rails become pills or circles only where their state benefits from it.

## Components

### Buttons

- **Tracking action:** a full-width three-stop gradient button (52px high, 16px corners) inside the timer; hover brightens and lifts it slightly, active returns it to rest, and disabled reduces opacity. Short windows reduce its height to 44px.
- **Primary:** a solid Action Teal control (38px minimum height, 11px corners) for routine confirmation; hover uses Action Teal Hover.
- **Secondary and ghost:** a pale bordered alternative and a quiet teal text action. Both receive a softer surface on hover.
- **Focus:** keyboard focus remains visible with a teal outline. Reduced-motion preference removes tracking-button and progress transitions.

### Fields

Inputs and selects have opaque white fills, cool borders, 11px corners, and a 39px minimum height. Focus shifts the border to teal and adds a soft teal ring. Dark appearance switches fields to deep-slate fills with bright text.

### Cards and badges

Standard cards use 20px corners, 20px padding, a fine border, and ambient lift. Compact Today controls and the weekly strip use the same family at tighter sizes. Success badges are small, fully rounded, and pale teal.

### Navigation

The floating dock shows Today, Insights, Coach, and Settings with icon and label together. Each item is at least 51px tall in the default layout. The selected item has a pale-teal fill and a teal icon and label; hover on inactive items is quieter. Coach stays visible but locked unless there is an authenticated session, an active paid entitlement, and `can_cloud_ai`; signing in alone does not unlock it.

### Timer and evidence

Today centers tracking state and tabular elapsed time above a goal rail, goal and streak labels, and the tracking action. Insights uses a weekly day strip, measured-time card, focus-category ratio rail, and task bars. The legacy focus-category time is categorized activity, not sustained-focus blocks. The local report may use AI or a rule-based fallback, so its visual treatment must not imply a guaranteed AI result.

### Monitoring consent

The bounded dialog keeps its heading and action row visible at 340 × 400px. Its details scroll independently; a “Read remaining details” cue appears while hidden text remains and disappears at the end. The same layout holds in light and dark appearances.

## Do's and Don'ts

### Do:

- Do use the 56px grid quietly behind readable surfaces.
- Do pair Manrope display type with Plus Jakarta Sans reading type across tabs.
- Do use the tracking gradient for tracking and selected first-run actions, with solid Action Teal for routine confirmation.
- Do preserve the 72px traffic-light clearance and keep the dock and consent actions reachable in narrow, short windows.

### Don't:

- Don't stretch the phone layout or its spacing directly into a wide desktop window.
- Don't turn categorized focus time into a sustained-focus claim or observed activity into a score-like visual.
- Don't present Coach as unlocked on sign-in alone or imply an unverified entitlement.
- Don't let the dock obscure the last controls or hide unread consent details behind fixed actions.
