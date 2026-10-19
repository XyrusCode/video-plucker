# Surface brief: Video Plucker Desktop

Scope: the whole application shell and its four destinations (Pluck, Queue,
Search, Settings) plus the terms modal and update banner.

## Audience, job, action

A person who already has the URL and wants a file on disk. They open the app to
start a download, to check on downloads they already started, to park a failure,
or to change a setting. Success is a file that plays. Frequency is high and
session length is long: the app stays open in the background all day.

Constraints that bind the design: no build step, no npm dependency in the
shipped app, a 920x680 default window with a 700x500 enforced minimum, Windows
first with macOS and Linux also shipped, and hide-to-tray on close and minimise.

## Direction contract

THESIS: A download manager is an instrument panel, not a document. Every pixel
earns its place by telling the user what is happening right now. This surface
refuses the centred column of cards that a phone layout produces in a desktop
window.

OWN-WORLD: A dark neutral ramp of four surfaces (`--bg`, `--surface`,
`--surface-hover`, `--overlay`) over 1px `--line` hairlines. One accent,
`--accent` `#FF2D55`, spent only on the active-rail marker, the primary button
fill, focus rings and the progress bar; `--accent-fill` `#E01E45` wherever
white text sits on it, because white on `#FF2D55` is 3.64:1 and fails. System
sans at a fixed 1.125 ratio. Radii 4/6/8/10 by size class. Elevation declared
once, as a hairline.

STORY: The user knows where everything is before noticing anything. The rail
says which of four places they are in. The table says what is downloading, how
far, how fast, and what to do about it. The status bar answers the three
questions they never think to ask: is the engine alive, what is moving right
now, and where does the file go.

FIRST VIEWPORT: A 38px title bar carrying the brand and, on Windows only, the
window controls. Below it a 208px rail on the left and the work area on the
right. The work area opens with the view title and its actions, then a sticky
table head (Name, Format, Size, Progress, Status) over 34px rows separated by
hairlines, with row actions revealed on hover. A 26px status bar closes the
window. The primary action, Pluck, sits in the options row of the Pluck view,
visually distinct as the only filled accent control on the screen.

FORM: The devtools workbench, candidate 5 of the grounded list, drawn by seed
`de9b365a6a7d57c1d27725a4dc606f99`. Signature move: the status bar as a
permanent instrument readout, the one place the app reports aggregate state
rather than per-item state.

## Raises carried from declined directions

- From the pocket timetable (rw-timetable-slide-rack): state is a mark in a
  fixed cell. Status lives in one column, always the same column, in the same
  seven words. It never moves and never changes shape.
- From the convention-circle catalog (pop-culture-shelf-doujin-event-catalog):
  the whole-cell grid. The poster grid and the episode picker reflow by whole
  cells, so a poster is never cropped to a half tile.
- From the lexicon spread (rw-lexicon-shoulder-and-plate): guide words at the
  head. The status bar carries what is happening, where the files land, and the
  running version, so orientation never depends on scrolling back up.

## Unresolved decisions

- Whether Windows-only frameless is worth the resize-edge risk. Resolved as
  Windows-only via `tauri.windows.conf.json`; macOS and Linux keep native
  decorations because removing macOS traffic lights is a usability regression,
  not a design win.
- Whether the poster grid in Search needs a filter row. Deferred: the app has
  no catalogue to filter yet, and a filter over nothing is decoration.