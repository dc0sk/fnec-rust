---
project: fnec-rust
doc: docs/gui-guide.md
status: living
last_updated: 2026-10-19
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

> **Solver note.** The **Solver** picker above the tabs chooses between:
>
> - **Hallén** — the default, and the right answer for most decks.
> - **MPIE** — the mixed-potential second solver, for near-ground currents over
>   finite ground, which Hallén does not model accurately. It also solves
>   junctions where three or more wires meet and closed loops — as does Hallén,
>   on its section graph, for voltage-driven decks without loads, networks or
>   perfect-ground contact.
>
> The choice applies to **every** tab — Solve, Sweep, Pattern and Currents — so
> the impedance on screen and the pattern beside it always come from the same
> solver. Changing it clears solved results, because the numbers showing were
> produced by the other one.
>
> On Hallén, a near-ground deck shows a warning naming the MPIE. A junction or
> loop with a straight run one segment long is refused on Hallén, naming the run
> and the MPIE (FND-251); the view still draws it, and the MPIE solves it.
> On MPIE, that warning is absent — the MPIE models those cases correctly. So
> is the Hallén caveat on a **radius taper** (a `GC` wire), or a radius step
> beside a one-segment run, which the MPIE models and Hallén does not; a step
> where two wires join Hallén models on its section graph (FND-252).
>
> The MPIE stamps `LD` loads. It has no solve for transmission lines or
> networks: a deck with `TL` or `NT` is **refused** on that solver rather than
> solved with the card ignored. Switch back to Hallén for those decks.
>
> The GUI also surfaces the refusals for ground cards fnec does not model (an
> undefined `GN` type or `GE` flag, `GE 1` without a `GN`, a wire on the ground
> without `GE 1`) and warnings for unsupported loads. The deck editor does not edit
> `GE`; change it in the deck text.
>
> A `GS` scale card stays where it is in the deck. A wire added in the table joins
> the wire block, which comes before a `GS` that follows the wires, so the new wire
> is scaled with the rest — as NEC reads that deck. The same holds for `GX`: a wire
> added before it is reflected too, so it appears twice (or four or eight times).
>
> A deck with a `GA` arc or `GH` helix draws in the viewport, but solves only with
> the MPIE solver selected: every node of an arc or helix is a bend, which the
> Hallén solve cannot model, and it says so.

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
- **Save deck** writes back over the file the document belongs to — the one it
  was loaded from, or the one the last **Save as…** wrote. That file is shown on
  the *Editing:* line under the editor, and it is **not** necessarily the deck
  path in the box at the top: retyping that box points the Load/Solve chrome
  somewhere else without moving the document. Until v0.18.0 `Save deck` used the
  box, so loading deck A, retyping the box to B and clicking Save truncated B
  with A's text (FND-103).
- **Save as…** opens a native save dialog and rebinds the document to the file it
  writes, so a later **Save deck** goes there too.
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
