# Video Plucker Desktop — UI contract

**This file is the canonical design contract for the desktop app.** It is
independent, not a mapping of another document. `src/styles.css` implements it,
`tests/ui-smoke.spec.js` enforces the parts that can be asserted from a
rendered page, and this document is the answer to any question of what the
desktop app should look like.

| | |
|---|---|
| Canonical | this file |
| Implementation | `src/styles.css`, `src/index.html`, `src/main.js` |
| Enforced by | `tests/ui-smoke.spec.js` |
| Product truth | `PRODUCT.md` |

---

## 0. What this document is not

There is also a `DESIGN.md` in the Android repository. That document describes a
phone app: a bottom navigation bar, thumb-sized targets, a single scrolling
column. This app is not that app in a larger window.

The two products share a name, a logo, an accent, a dump file format, and
nothing else about how they look. The Android app is operated with a thumb, one
hand, at arm's length, usually interrupted. This app is operated with a mouse
and a keyboard, in a 920x680 window, while something else is happening in the
foreground, often for hours. Those are different objects and they get different
contracts.

**Why 4.9.0 replaced the shared contract.** Version 4.8.0 made the desktop app
implement the phone's rules: pill navigation, a 50px gradient primary button, a
centred 860px column, a rounded card around every row and every settings group,
an accent dot beside each section heading. Every one of those is a Material
idiom that has no desktop counterpart. In a desktop window they read as a web
page pretending to be an application, which is what users reported.

Rules that survived from the previous contract, because they were never the
problem: the accent is `#FF2D55`, one accent is enough for the whole app, and a
failure is never rendered in the accent colour.

---

## 1. Principles

1. **Instrument, not document.** The user is looking at something that changes
   on its own. Everything on screen is either a control, a navigation item, or a
   reading. Nothing decorates.
2. **One accent, four jobs.** The accent marks what is active, what is primary,
   where the keyboard focus is, and how far along a download is. It is never
   used for a heading, a border, a fill on a row, or decoration.
3. **Boxes carry grouping, not depth.** A 1px hairline separates. A surface
   changes tone. There are no shadows except on things that float above the
   window (the modal and the overflow menu).
4. **Words over shapes.** A status is a word in a fixed column. A download's
   type is its filename, not a coloured tile. Never encode one thing twice.
5. **Everything is reachable by keyboard.** The whole app is operable without a
   mouse, and focus is always visible.

---

## 2. Colour

Restrained strategy: a four-step neutral ramp with one accent. The app sits open
all day beside other windows on a desktop, so the ground is dark and desaturated
and the accent is the only saturated thing on screen.

| Token | Value | Used for |
|---|---|---|
| `--bg` | `#0F1115` | The content area behind everything |
| `--surface` | `#14171C` | Rail, title bar, status bar, modal base |
| `--surface-hover` | `#191D23` | Row hover, input fill, menu base |
| `--overlay` | `#1B2027` | Modal and overflow menu |
| `--line` | `#242A32` | Hairlines, table dividers, input borders |
| `--line-strong` | `#303845` | Dividers that need more separation |
| `--text` | `#E6E9EF` | Primary text |
| `--text-2` | `#9AA4B2` | Secondary text, table heads, placeholders |
| `--text-3` | `#7C8794` | Meta, timestamps, disabled labels |
| `--accent` | `#FF2D55` | Active rail marker, focus ring, progress fill, active tab underline |
| `--accent-fill` | `#E01E45` | Primary button background, and nothing else |
| `--accent-soft` | `rgba(255, 45, 85, 0.12)` | Active rail row fill, selected chip |
| `--ok` | `#3FB950` | Completed |
| `--warn` | `#E3A008` | Paused, queued, partial |
| `--error` | `#FF7B72` | Failed, and failure text the user must read |

### Why there are two reds

White text on `--accent` is 3.64:1 and fails WCAG AA for body text. White on
`--accent-fill` is 4.73:1 and passes. So `--accent` stays the exact brand red
and carries marks that are not text (the rail marker, the progress fill, the
focus ring), while `--accent-fill` is the only value white text ever sits on.

`--error` is a third red, lighter and more orange than the accent. It is
reserved for failure text so a failure never reads as a button. Destructive
buttons are `--accent-fill` like any other primary, because a destructive
action in this app (clear finished, delete a cookie set, dismiss a row) is an
ordinary small action, not an alarm.

**No colour is used without a word beside it.** Status is always the word plus
the colour, never the colour alone.

---

## 3. Typography

One family, the platform UI sans, because this is an app and the platform's
default is the one face already tuned for it at every size. Fixed rem scale at a
1.125 ratio. No `clamp()`, no fluid type: the window is a fixed object, not a
web page that reflows with the viewport.

| Role | Size / weight | Notes |
|---|---|---|
| View title | 17px / 600 | `-0.01em` |
| Body | 13px / 400 | The default everywhere |
| Label | 12px / 500 | Field labels, rail items, buttons |
| Table head | 11px / 600 | `--text-2`, `-0.02em` |
| Meta | 11px / 400 | `--text-3` |

**No uppercase, no letter-spacing, anywhere.** This includes table column heads.
The previous contract banned it in prose and the stylesheet shipped it anyway in
`.job-status` and `.pl-state`; the smoke suite now asserts that no computed
`text-transform` is `uppercase`.

Numerals in the status bar, the table's size and rate columns, and the progress
percentage are `font-variant-numeric: tabular-nums`, so digits do not shift as
they tick.

---

## 4. Space, shape, and the frame

4px grid. Density is a feature: a user watching four downloads at once wants all
four rows visible without scrolling.

| Token | Value | Used for |
|---|---|---|
| `--h-titlebar` | 38px | Title bar height |
| `--h-rail-item` | 30px | Rail item height |
| `--h-table-head` | 28px | Table header row |
| `--h-row` | 34px | Table body rows |
| `--h-statusbar` | 26px | Status bar |
| `--w-rail` | 208px | Rail width |
| `--w-rail-collapsed` | 56px | Rail width below 880px |
| `--space-1` / `--space-2` / `--space-3` / `--space-4` | 4 / 8 / 12 / 16px | General rhythm |
| `--space-6` / `--space-8` | 24 / 32px | Between blocks |

Radii by size class, never by taste:

| Token | Value | Used for |
|---|---|---|
| `--radius-input` | 4px | Inputs, selects, buttons |
| `--radius-panel` | 6px | Menus, banners |
| `--radius-modal` | 12px | The terms modal |

**Pills are for small controls only.** Rail items, table rows, settings groups
and section labels are not pills. There is no pill navigation.

**Elevation is declared once.** Flat surfaces use a 1px `--line` border and no
shadow. Only the two things that genuinely float above the window, the terms
modal and the row overflow menu, carry a shadow with a real offset and a soft
blur. A border under a wide shadow is the ghost card and is banned.

---

## 5. The shell

```
┌───────────────────────────────────────────────────────────────┐
│ ▣ Video Plucker                                        − □ ×   │ 38
├──────────┬────────────────────────────────────────────────────┤
│ Pluck    │  Downloads                         Clear finished  │
│ Queue    │  Name          Format  Size    Progress   Status    │ 28
│ Search   │  ─────────────────────────────────────────────────  │ hairline
│ Settings │  A Video       1080p   642 MB  ▓▓▓▓░░░    Plucking │ 34
│          │                                                    │
├──────────┴────────────────────────────────────────────────────┤
│ ● Ready   2 active · 4.1 MB/s    C:\Users\me\Videos    v4.9.0  │ 26
└───────────────────────────────────────────────────────────────┘
```

### Title bar (`38px`)

Brand mark and wordmark on the left, window controls on the right. On Windows
the whole bar is a drag region and carries minimise, maximise and close. On
macOS and Linux the window keeps its native decorations and this bar shows the
brand only, because removing the macOS traffic lights costs the user the
double-click-to-maximise gesture and the standard close behaviour for no gain.

The title bar is not a toolbar. There are no app actions in it. An app that
puts actions in its title bar is a web page in a window.

### Rail (`208px`, collapsing to `56px` below 880px`)

Four destinations, icon and label, in the order they are used. Active item:
`--surface-hover` fill, a 2px `--accent` bar on the leading edge, `--text`
label. Inactive: `--text-2`, no fill. The icon rail below 880px is the same four
items with the label hidden and the icon centred; there is no separate code path,
only a width and a media query.

The rail is collapsed rather than hidden because the app has four destinations,
not four dozen. Losing the labels at the app's minimum width is a real loss; it
is the right trade, and the icons carry tooltips via `title`.

### Status bar (`26px`)

The signature move and the one part of this contract that no mobile counterpart
has. Four readings, left to right:

1. Engine state, as a coloured dot plus a word (`Ready`, `Updating`, `Error`).
2. Aggregate activity: active count and summed throughput, or a quiet
   `Idle` when nothing is running.
3. The destination folder, elided from the left so the visible part is the tail.
4. The running app version, right-aligned.

It is always visible. It never scrolls away. It is how the user orients without
scrolling back up, and it is the only place aggregate state is reported, because
per-item state belongs to the row it belongs to.

---

## 6. Views

### 6.1 View header

Title (`17px/600`) on the left, the view's actions on the right, on one 44px
line. No subtitle: the four destinations are named in the rail, and a subtitle
that repeats the title is a label with no information in it.

### 6.2 Tables

Downloads and Queue are tables. Not cards, not a list of rounded panels.

- A sticky table head (`28px`, `--text-2`, `11px/600`) with the column names.
- Body rows `34px`, separated by `--line` hairlines, no fill, no radius.
- Hover raises the row to `--surface-hover`.
- A row carries: name, format, size, progress, status, actions. Every column is
  a field the app actually has. Do not add a column that is not in the payload.
- Actions are revealed on row hover and on row focus-within, so a resting table
  shows no button clutter. One primary action stays visible; the rest sit in a
  `…` overflow menu.
- Right-aligned columns are numeric: size, speed, ETA, percent.
- A playlist row expands in place into item rows, indented, with the same
  hairline treatment one level in.

### 6.3 The overflow menu

A `…` button opens a positioned menu: `--overlay` fill, `--line` border, the one
real shadow in the app, 6px radius. It closes on outside click and on `Escape`.
The Popover API is not used; it is unreliable on older WKWebView, and this app
ships a `.dmg`.

### 6.4 Settings, two panes

A section list on the left (`Downloads`, `Updates`, `Cookies`, `Backup`,
`About`), the selected section on the right. Control rows are label and
description on the left, the control on the right, separated by hairlines.
Sections are not cards. There is no icon tile beside a section heading: the
heading and its description are enough, and the coloured tiles from 4.8.0 were
mobile decoration.

### 6.5 Search

The poster grid is content, not data, so it stays a grid. Posters are `2:3` and
reflow by whole cells, so a poster is never cropped to a half tile. The detail
view is an inline pane under the grid, not a modal, because picking an episode
is not a task that needs protected focus.

### 6.6 Empty states

An empty state names what will fill the space and how to start it. "Nothing
downloaded yet. Paste a link above and press Analyze." is correct. "No data" is
not.

### 6.7 The analysis grid

The link field takes one link per line. One link is analysed into the detail
card under the field, because one link does not need a grid. Several links are
analysed into a grid of tiles, one per link, a few at a time.

- Tiles are `16:9` and reflow by whole cells, so a thumbnail is never cropped
  to a half tile. The thumbnail is taken out of the flow, so the tile's shape
  comes from the ratio and not from the image.
- A tile carries the thumbnail, the title, the site and the length. That is
  every field the analysis returned, and nothing else.
- A link that could not be read says so on its own tile, in the thumbnail's
  place, in the failure red. The one action that can fix it, Refetch, is on
  the same tile.
- The grid is content, so it is a grid and not a table of rows. It sits under
  the field, and the field's own controls, the format and the login, still
  apply to every tile in it.
- A tile that has been plucked leaves the grid: it is in the downloads table
  now, and keeping it in the grid would say it was waiting.

---

## 7. Controls

Every control ships all of: default, hover, focus-visible, active, disabled.

| Control | Treatment |
|---|---|
| Primary | `--accent-fill` fill, white `12px/600`, 4px radius, 28px tall |
| Secondary | `--surface-hover` fill, `--line` border, `--text` label |
| Quiet | transparent fill, `--text-2` label, `--surface-hover` on hover |
| Destructive | as quiet, but the label and its hover go to `--error` |
| Input / select | `--surface-hover` fill, `--line` border, 4px radius, 28px tall, `--accent` focus ring at 3px |
| Toggle | 38x22 track, `--accent` when on, `--line-strong` when off, white thumb |
| Table row action | 24px square, transparent, `--text-2`, `--surface-hover` on hover |

Icon buttons are drawn SVG at a single stroke weight. **No Unicode glyphs as
icons.** `▸`, `✕` and `←` are banned in this document; 4.8.0 shipped all three.

Press feedback is `transform: scale(0.98)` on `:active`, 120ms. Buttons do not
scale on hover, because a mouse-driven hover that moves geometry reads as a
mis-click.

---

## 8. Motion

120ms for colour, 160ms for transform, both `cubic-bezier(0.2, 0, 0, 1)`.
Exponential ease-out, starting from an already-visible default, so nothing
arrives late and nothing is hidden by its own animation.

There is one authored moment: the row overflow menu, which fades and lifts 4px
over 160ms. Everything else is a colour or opacity change. There is no
page-load choreography, no staggered entrance, and nothing animates that does
not report a state change.

`prefers-reduced-motion: reduce` removes transform and shortens every duration
to 0.01ms. The menu still opens; it simply does not travel.

---

## 9. Browser surfaces

The parts of the window the app did not draw still carry the design:

- Text selection is `--accent-soft` on `--text`.
- The caret is `--accent`.
- Scrollbars are 10px, `--line` track, `--line-strong` thumb.
- Focus is a 3px `--accent-soft` ring with a 1px `--accent` inner edge, visible
  on every focusable element.
- Tabular numerals everywhere a number ticks.
- Links carry a 2px underline offset.

---

## 10. Anti-patterns

Each of these shipped in 4.8.0 or earlier and is removed. Do not reintroduce.

| Removed | Why it is wrong |
|---|---|
| Pill navigation | A phone idiom. Desktop destinations belong in a rail. |
| A centred `max-width` column | A web-page layout. A desktop window has space to spend. |
| A gradient primary button | Decoration on the one control the user must find. |
| A 50px tall primary button | A touch target in an app operated by a mouse. |
| A card around every row and every settings group | Cards are grouping, and they were carrying all the grouping. |
| An accent dot beside each section heading | Mobile decoration with no information in it. |
| Icon tiles in settings headers | The heading and its description already say what the group is. |
| `text-transform: uppercase` with tracking | The loudest thing on a screen with no other hierarchy. |
| Unicode glyphs as icons | `▸`, `✕`, `←` render differently per platform and are not icons. |
| `backdrop-filter` over an opaque background | Nothing renders behind it, so it costs a compositing layer and blurs nothing. |
| A card that is also a row | Two containers doing one job. |
| A modal for a task that needs no protected focus | Episode selection and result detail are inline. |

---

## 11. Accessibility

- 4.5:1 for body and placeholder text, 3:1 for large text and for non-text
  state indicators. The values in section 2 are chosen against these ratios, not
  picked by eye.
- Status is a word plus a colour, never a colour alone.
- Every icon-only control carries an `aria-label` naming the action.
- The rail sets `aria-current="page"` on the active destination.
- The whole app is operable from the keyboard; the overflow menu traps nothing
  but closes on `Escape` and returns focus to its trigger.
- Usable at the 700x500 minimum window and at 150% display scaling.

---

## 12. Enforcement

`tests/ui-smoke.spec.js` asserts what can be read off a rendered page: the
tokens in section 2, one active rail item, a table head on every table view, the
status bar carrying the version, no computed `uppercase`, no gradient on any
button, and a row height within the section 4 bound.

A screenshot catches what looks wrong. A token assertion catches what drifted.
Both run on every PR, through `npm install && npx playwright test`, against a
static server with a stubbed Tauri bridge. No Rust toolchain is needed.

`scripts/shoot-before-after.mjs` captures the four views at the real window size
and at the minimum, through the same stub and viewport on both sides, so the
pair is comparable rather than two different setups.