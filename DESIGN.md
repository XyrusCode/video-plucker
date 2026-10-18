# Video Plucker Desktop — UI contract

**The canonical contract is `DESIGN.md` in the Android repository**
(`XyrusCode/video-plucker-android`). Android owns the reference implementation;
this file records how each rule lands in CSS so a reviewer can check one side
against the other without opening both apps.

When the two disagree, the canonical document wins and this mapping is the bug.

| | |
|---|---|
| Canonical | `video-plucker-android/DESIGN.md` |
| Android implementation | `ui/theme/Theme.kt`, `ui/components/*.kt` |
| Desktop implementation | `src/styles.css` |
| Enforced by | `tests/ui-smoke.spec.js` |

---

## Token mapping

Section 2 of the canonical document. Every value in `styles.css :root`.

| Canonical | Value | CSS variable |
|---|---|---|
| `Bg` | `#0D0D0F` | `--bg` |
| `Panel` | `#1C1C1E` | `--panel` |
| `Panel2` | `#2C2C2E` | `--panel-2` |
| `BorderCol` | `#3A3A3C` | `--border-col` |
| `GlassBorder` | 10% white | `--glass-border` |
| `TextCol` | `#F5F5F7` | `--text` |
| `TextDim` | 60% white | `--text-dim` |
| `Accent` | `#FF2D55` | `--accent` |
| `AccentDim` | `#BF1A3D` | `--accent-dim` |
| `AccentGlow` | `#FF375F` | `--accent-glow` |
| `AccentSoft` | 10% accent | `--accent-soft` |
| `AccentSoftStrong` | 20% accent | `--accent-soft-strong` |
| `Ok` | `#30D158` | `--ok` |
| `Warn` | `#FF9F0A` | `--warn` |

### Error is not a token

`--error: #ff453a` is desktop-only, for failure text the user has to read.
Destructive actions use `--accent`, per section 2 of the canonical document. Two
reds would mean neither reads as the brand.

### What changed in 4.8.0

The accent was `#0a84ff` (Apple blue) throughout, with a pink gradient hardcoded on
the logo square and no token behind it. That made the app's only brand colour a
one-off that appeared nowhere else. Both are gone: the accent is now `--accent`
and the logo consumes it like everything else.

## Spacing and radius

Section 3. `--space-screen` 20px, `--space-section` 8px, `--space-item` 12px,
`--space-card` 14px, `--radius-row` 12px, `--radius-card` 14px,
`--radius-thumb` 8px, `--radius-chip` 10px.

Before this there were no spacing or radius tokens at all: eleven distinct radius
literals and eighteen spacing literals, all hardcoded.

## Component mapping

| Canonical | Kotlin | CSS |
|---|---|---|
| `BrandTopBar` | `AppTopBar.kt` | `.brand-bar` |
| `ScreenHeader` | `AppChrome.kt` | `.screen-header` / `.screen-title` / `.screen-sub` |
| `SectionHeader` | `SectionHeader.kt` | `.section-header` / `.section-dot` / `.section-title` / `.section-desc` |
| `SectionLabel` | `AppChrome.kt` | `.section-label` |
| `FlatRow` | `AppChrome.kt` | `.flat-row` |
| `GlassCard` | `GlassCard.kt` | `.card` |
| `IconChip` | `SettingsScreen.kt` | `.icon-chip` |
| `PillChip` / `PillRow` | `AppChrome.kt` | `.pill-chip` / `.pill-row` |
| `GlowButton` | `GlowButton.kt` | `button.primary` |
| `GlassButton` | `GlowButton.kt` | `button` (bare) |
| Toggle switch | `SettingsScreen.kt` | `.toggle` |
| Row icon tile | `HistoryScreen.kt` | n/a, no equivalent row on desktop |

### Two adaptations worth knowing

**`FlatRow` versus `GlassCard`.** The canonical document uses `FlatRow` for list
rows and `GlassCard` for grouped settings containers. Desktop uses `.flat-row` for
job cards, queue rows and cookie rows, and `.card` for the five settings groups.

**`GlassButton`'s outline is load-bearing.** The canonical button is an
`OutlinedButton` at 6% accent fill. On a near-black panel a 6% tint alone is
unreadable, so the 20% accent border is what makes the secondary buttons legible.
It is not decoration; do not remove it.

## What desktop does not have

| Canonical | Why not |
|---|---|
| Browser, Anime, Saved tabs | Android-only features. The desktop app has no in-app browser and no Bookmarks implementation. |
| History tab | Desktop tracks active and finished downloads through `plucks.json`, not a MediaStore album. |
| Bottom nav | Desktop is a top `PillRow`; the phone's nav is at the bottom because of the thumb. |
| `IconChip` deviation note | `surfaceContainer` has no CSS equivalent here; `.icon-chip` uses `--panel-2`, which is what the canonical document says new code should use. |

## Layout

Section 7. One column, `max-width: 860px`, `--space-screen` from each edge.
Downloads live inside the Pluck tab, matching the phone. Before 4.8.0 they sat
outside the view system and rendered on every tab including Search and Settings.

The `PillRow` scrolls horizontally rather than dividing evenly, for the reason in
section 5 of the canonical document.

## Motion

Section 6. `--ease: cubic-bezier(0.2, 0, 0, 1)`, `--dur-color: 150ms`,
`--dur-transform: 120ms`. Button press scale is `0.97`, matching `MotionSpring`.
`prefers-reduced-motion` removes both.

## Anti-patterns removed in 4.8.0

From section 8 of the canonical document. Do not reintroduce any of them.

| Removed | Was |
|---|---|
| `backdrop-filter` on every panel | Rendered over a flat `#0d0d0f` body, so it blurred nothing and cost a compositing layer on six separate rules |
| Seven copies of the surface block | `.chrome`, `#meta-card`, `#result-detail`, `.job-card`, `.queue-row`, `.settings-section`, `#cookie-manager` each re-declared the same four properties |
| `.glossy-btn` | Applied to 20 elements with no CSS rule behind it since 4.7.0 |
| Hardcoded `#ff375f`/`#ff2d55` logo gradient | The app's only brand colour, untokenised |
| `.ep-row` split in two | Declared 480 lines apart, one for layout and one for a border |
| Letter-spaced uppercase labels | `.badge`, `.pl-state`, `.job-status`, `.version-label` |
| `.clear-history` on a non-destructive button | "Start all" inherited `color: var(--error)` on hover |
| A second auto-update toggle | One in the footer, one in Settings, synced by hand |

## Bugs fixed in 4.8.0

- Cookie-manager errors rendered into `#analyze-error`, which only exists on the Pluck tab, so a failure on any other tab was invisible.
- "Start all" iterated the store in insertion order while the list displayed `parkedAt` order.
- A job card's controls were bare siblings in a `space-between` row with no wrap; more than three visible overflowed the card.
- `restoreInterruptedPlucks` and the queue both emptied state without a rendered empty state.

## Testing

`npm install && npx playwright test` runs the smoke suite against a static server
with a stubbed Tauri bridge. It asserts the token values, that every
`getElementById` in `main.js` resolves, that downloads stay inside the Pluck tab,
and that no view is missing a title or subtitle.

The suite has a `screenshots` test whose attachments double as the after-shots for
any UI change, so the shot harness and the regression harness cannot drift apart.

Rust unit tests run with `cargo test` from `src-tauri`.
