---
project: fnec-rust
doc: docs/gui-guide.md
status: living
last_updated: 2026-07-13
---

# fnec-gui user guide

`fnec-gui` is the desktop workbench for the fnec antenna-modelling engine — a
GPU-accelerated, rotatable/zoomable 3-D view beside task-oriented panels for
solving, sweeping, plotting patterns, inspecting currents, and editing the deck.

Launch it from the workspace root:

```sh
cargo run -p nec-gui
```

## Window layout

The window is a single **resizable split** (xnec2c-style):

- **Left — control pane.** The deck inputs and a tab bar: **Solve · Sweep ·
  Pattern · Currents · Edit**. The active tab's results appear below.
- **Right — 3-D viewport.** Always visible, regardless of the active tab.

Drag the divider between the panes to rebalance them.

At the top of the control pane:

- **Deck file** — path to a `.nec` deck. Type it, or click **Browse…** for a
  native file picker.
- **Vars file** — optional `.toml`/`.json` variable map for decks that use
  `$VAR` tokens. **Browse…** picks one.

## The 3-D viewport

| Action | Gesture |
|:-------|:--------|
| Orbit | left-drag |
| Zoom | mouse wheel |
| Pan | middle- or right-drag |
| Reset view | **Reset view** button |

Controls above the view:

- **Load geometry** — parse the deck and draw its wires (no solve). The camera
  frames the antenna automatically.
- **Solve currents** + **Color by |I|** — solve and paint each wire by current
  magnitude (hot = feedpoint peak, cold = tips); a legend shows the mA range.
- **Solve pattern** + **Show pattern** — compute the full-sphere far field and
  overlay a translucent 3-D radiation lobe.
- **Axes** / **Grid** — toggle the xyz axis triad and the z=0 ground grid.

## Solve tab

Click **Solve** to run a single-frequency solve of the deck; the feedpoint
frequency, resistance, reactance, and |Z| are shown.

> **Solver note.** The GUI runs the **Hallén** solver. For geometries it does not
> model accurately — junctions where three or more wires meet, closed loops, and
> near-ground currents over finite ground — the Solve tab shows a ⚠ warning and
> the numbers should not be trusted. Solve those with the command line, which has
> the mixed-potential second solver:
>
> ```sh
> fnec --solver mpie [--ground-solver sommerfeld] your-deck.nec
> ```
>
> The GUI also surfaces ⚠ warnings for deferred ground models (treated as free
> space) and unsupported loads.

## Sweep tab

Enter a **Start / End / Step** (MHz) and click **Run Sweep**. Results **stream
in live** — the chart and table fill as each frequency solves.

- **Chart** — SWR or |Z| against frequency, with a red **frequency cursor**.
- **SWR / |Z|** picker — choose the plotted quantity.
- **Frequency slider** — scrub the cursor across the swept range; the readout
  shows the exact frequency, Z, |Z|, and SWR at the nearest point.
- The result **table** is sortable by any column.

## Pattern tab

Enter an azimuth **φ** and click **Run Pattern** for an elevation-plane slice
(θ = 0–180°); the gain-vs-θ table has a bar for each angle. For the full 3-D
lobe, use **Solve pattern** in the viewport instead.

## Currents tab

Click **Run Currents** for the per-segment current-distribution bar chart. For
the 3-D coloured view, use **Solve currents** in the viewport.

## Edit tab — the visual deck editor

Click **Load deck into editor** to turn the deck into editable tables. Every
valid edit **previews live** in the 3-D viewport.

**Wires (GW).** One row per wire — Tag, Segments, x1/y1/z1, x2/y2/z2, radius.

- **+ Add wire** appends a wire; **Del** removes one.
- Invalid input (bad number, radius ≤ 0, zero-length wire) shows a ⚠ reason and
  keeps the last good preview.

**Sources & environment (EX / GN / LD / FR).** An editor per control card in the
deck; the enumerated fields (ground type, load type, frequency step type) are
pick-lists. Use **+ Ground / + Load / + Source** to insert a card a deck doesn't
have yet, and each row's **Del** to remove one.

**Actions.**

- **Undo / Redo** (or `Ctrl+Z` / `Ctrl+Shift+Z` / `Ctrl+Y`) — full edit history;
  typing a value coalesces into one undo step.
- **Save deck** writes back over the loaded path; **Save as…** opens a native
  save dialog.
- **Apply + Solve** solves the edited in-memory deck and shows the impedance,
  without saving first.

## Session persistence

fnec-gui remembers your last session — deck and vars paths, the sweep range and
chart metric, the camera pose, and the axes/grid toggles — in
`$XDG_CONFIG_HOME/fnec-gui/session.toml` (or `~/.config/fnec-gui/session.toml`).
Reopening the app restores them; load the deck again to redraw its geometry.

## Keyboard shortcuts

| Shortcut | Action | Where |
|:---------|:-------|:------|
| `Ctrl+Z` | Undo | Edit tab |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Redo | Edit tab |

(On macOS, `Cmd` substitutes for `Ctrl`.)
