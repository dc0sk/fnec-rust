---
project: fnec-rust
doc: docs/gui-redesign-plan.md
status: living
last_updated: 2026-07-10
---

# GUI redesign — GPU-accelerated 3D antenna workbench (iced 0.13)

An action plan for evolving `apps/nec-gui` from its current four text tabs
(Solve / Sweep / Pattern / Currents, ~1360 LOC) into a comprehensive desktop
workbench: a rotatable/zoomable/pannable **wgpu 3D view** of the wire geometry,
a **3D radiation-pattern lobe** overlay, **current-magnitude coloring** on the
wires, and **visual deck editors** (GW wires, EX excitation, GN ground, LD
loads, FR sweep) with re-solve — 4nec2-class capability in an xnec2c-style
single window.

Every phase below is one branch → PR → squash-merge increment with explicit
acceptance gates, per the house workflow. Phase 0 is specified to
start-coding detail.

**Grounding — files this plan is based on** (all read, not assumed):

- `apps/nec-gui/src/{main.rs,app_state.rs,solve.rs,lib.rs}` — current app;
  headless `AppState::apply(&Message)` state machine, `Task::perform`
  background solves, per-tab `*Phase` enums.
- `apps/nec-gui/tests/gui_smoke.rs` — headless CI gate pattern (state machine +
  solve pipeline, no display).
- `crates/nec_solver/src/geometry.rs` — `Segment { start, end, midpoint,
  direction, length, radius, tag, tag_index, global_index }`, `build_geometry`,
  `GroundModel`, `wire_endpoints_from_segs`.
- `crates/nec_solver/src/farfield.rs` — `compute_radiation_pattern(&[Segment],
  &[Complex64], freq_hz, &[FarFieldPoint], &GroundModel) -> Vec<FarFieldResult>`
  (per-point `gain_total_dbi`, θ/φ components, −999.99 dB floor sentinel).
- `crates/nec_accel/src/{wgpu_device.rs,gpu_kernels.rs}` — compute-only wgpu
  **29.x** device (`new_without_display_handle`), Hallén-FR kernels.
- `crates/nec_model/src/card.rs` — `GwCard`, `ExCard`, `GnCard`, `LdCard`,
  `FrCard` field sets (the editors' data model).
- `crates/nec_project/src/lib.rs` — project manifest scope (`SolverConfig`,
  `NamedRun`) for later save/load integration.
- `Cargo.toml` / `Cargo.lock` — workspace pins `wgpu = "29"`; iced **0.13.1**
  resolves `iced_wgpu 0.13.5` on **wgpu 0.19.4** (both wgpu versions already
  coexist in the lock file today).
- `docs/roadmap.md` (PRT-004 usability parity), `docs/mpie-solver-scope.md`
  (house doc style).

---

## 1. Vision + scope

### What the redesigned GUI is

A single-window antenna workbench: load or edit a NEC deck, see the wire
structure in 3D immediately, solve off-thread, and see feedpoint impedance,
current distribution painted onto the wires, and the 3D far-field lobe overlaid
on the geometry. Sweeps stream progress and fill an SWR/|Z| plot; a frequency
slider re-solves interactively across the swept band (xnec2c's signature
workflow). Editors write back to the deck model and re-solve on demand.

### Layout: single window with resizable panes (xnec2c-style) — recommended

4nec2 (studied via <https://www.qsl.net/4nec2/screenshot.htm>) uses ~6 floating
MDI windows: main/overview, NEC editor, geometry-edit table, 3D viewer +
current display, far-field pattern window, SWR sweep plots, optimizer. xnec2c
packs structure/currents/pattern/plots into one window with a frequency
control.

**Recommendation: xnec2c-style single window**, panes implemented with iced's
built-in `iced::widget::pane_grid` (resizable/draggable splits). Rationale:

- iced 0.13 multi-window (`iced::daemon`) is young, complicates the single
  `AppState` + `Message` architecture the app already has, and breaks the
  headless-test story that gates this crate in CI.
- 4nec2's floating windows are an MDI-era artifact; its *information design*
  (geometry table editor, pattern + geometry side by side, SWR plots) carries
  over fine into docked panes.
- `pane_grid` gives 80 % of docking (user-resizable splits) with zero new
  dependencies.

### Target layout mockup

```
┌────────────────────────────────────────────────────────────────────────────┐
│ fnec-gui   [Open…] [Save] [Solve ▶] [Sweep ▶]     status: Done — 14.000 MHz│
├───────────────┬─────────────────────────────────────┬──────────────────────┤
│ SIDEBAR       │        3D VIEWPORT (shader widget)  │  RESULTS             │
│ deck cards:   │                                     │  Z = 73.1 + j1.5 Ω   │
│ ▸ Wires (GW)  │        wire geometry,               │  VSWR(50Ω) = 1.47    │
│ ▸ Source (EX) │        segments colored by |I|,     │  Gmax = 2.14 dBi     │
│ ▸ Ground (GN) │        translucent 3-D gain lobe,   │ ┌──────────────────┐ │
│ ▸ Loads (LD)  │        ground grid + xyz axes       │ │ SWR / |Z| sweep  │ │
│ ▸ Freq (FR)   │                                     │ │ plot (canvas)    │ │
│ ──────────────│   drag = orbit   wheel = zoom       │ └──────────────────┘ │
│ editor form   │   middle-drag / shift-drag = pan    │ ┌──────────────────┐ │
│ for selected  │                                     │ │ 2-D polar slice  │ │
│ card row      │                                     │ │ (canvas)         │ │
│ [Apply+Solve] │                                     │ └──────────────────┘ │
├───────────────┴─────────────────────────────────────┴──────────────────────┤
│ freq ◀───────●────────▶ 14.150 MHz   ☑ Currents  ☑ Pattern  ☑ Grid  ⟲ Reset│
└────────────────────────────────────────────────────────────────────────────┘
```

Three `pane_grid` panes (sidebar / viewport / results) + fixed top toolbar and
bottom control strip. The existing Solve/Sweep/Pattern/Currents tab *logic*
survives as the results-pane content and the background task plumbing; the tabs
themselves dissolve into this layout in Phase 5.

### Out of scope (this plan)

Optimizer UI, Smith chart, near-field 3D visualization, multi-window, NEC text
editor with syntax highlighting (the sidebar editors + external editor cover
it), GT/GA/GH geometry cards in the editor (viewer renders whatever
`build_geometry` produces; the editor covers straight GW wires first).

---

## 2. Architecture

### Module layout (`apps/nec-gui/src/`)

```
main.rs              iced entry: application(), toolbar, pane_grid composition
lib.rs               pub mod list (unchanged role)
app_state.rs         headless state machine (extended; stays iced-free EXCEPT
                     the pure-math viewport camera type it embeds)
solve.rs             solver bridge (extended: full-sphere pattern, currents
                     with geometry, streamed sweeps)
model_doc.rs         editable deck document: Vec<WireRow>/ExRow/... of String
                     fields ↔ validated nec_model cards; dirty tracking
deck_write.rs        NEC card → text serializer (none exists in the workspace
                     today — grep confirms no writer in nec_model/nec_parser)
viewport/
  mod.rs             Scene: iced::widget::shader::Program<Message> impl
  camera.rs          orbit camera (pure glam math, unit-tested headlessly)
  primitive.rs       ScenePrimitive: shader::Primitive impl (prepare/render)
  pipeline.rs        wgpu pipelines + depth texture, cached in shader::Storage
  mesh.rs            Segment list → wire vertices; gain grid → lobe mesh;
                     colormap (pure, unit-tested headlessly)
  shaders/           wires.wgsl, pattern.wgsl, grid.wgsl (WGSL for wgpu 0.19)
editors/
  mod.rs wires.rs excitation.rs ground.rs loads.rs frequency.rs
panels/
  results.rs sweep_plot.rs pattern2d.rs   (canvas-based 2-D plots)
```

### State + message model

Extend the proven pattern (`AppState::apply(&Message)` pure, iced binary thin):

```rust
// app_state.rs additions (illustrative)
pub struct ViewportState {
    pub camera: OrbitCamera,          // pure math, in viewport/camera.rs
    pub show_currents: bool,
    pub show_pattern: bool,
    pub show_grid: bool,
    pub scene_rev: u64,               // bumped when meshes change → re-upload
}
pub enum Message {
    // …existing…
    Viewport(ViewportMsg),            // Orbit{dx,dy} | Zoom{steps} | Pan{dx,dy} | Reset
    GeometryLoaded(Result<SceneGeometry, String>),  // segments for the 3-D view
    Pattern3dComplete(Result<PatternGrid, String>),
    SweepProgress(SweepPoint),        // streamed
    Editor(EditorMsg),                // per-card-type field edits + Apply
    FreqSliderMoved(f64),
    PaneResized(pane_grid::ResizeEvent), // the one iced type allowed in state
}
```

Camera lives in `AppState` (not widget-local) so `gui_smoke.rs`-style headless
tests can drive `Message::Viewport(...)` and assert view matrices. Only
transient drag bookkeeping (button-down flag, last cursor position) lives in
the shader widget's `Program::State`.

### The wgpu 3-D viewport — concrete iced 0.13 mechanism

Verified against docs.rs for iced 0.13.1 / iced_wgpu 0.13.5 (this is the crux;
see §3 for the full feasibility findings):

1. `Scene` implements `iced::widget::shader::Program<Message>`:
   - `type State = DragState;` (Default)
   - `type Primitive = ScenePrimitive;`
   - `draw(&self, state, cursor, bounds) -> ScenePrimitive` — clones camera
     matrix + Arc'd mesh handles into the primitive (cheap; buffers are only
     re-uploaded when `scene_rev` changes).
   - `update(&self, state, event, bounds, cursor, shell) -> (Status, Option<Message>)`
     — translates mouse events into `Message::Viewport(...)`.
2. `ScenePrimitive` implements `shader::Primitive`
   (`Debug + Send + Sync + 'static`):
   - `prepare(&self, device, queue, format, storage, bounds, viewport)` —
     first call creates pipelines + a private **depth texture** and stores them
     in `Storage` (type-keyed store); every call writes the camera uniform and
     (on `scene_rev` change) vertex/index buffers via `queue.write_buffer`.
   - `render(&self, encoder, storage, target, clip_bounds)` — one render pass
     onto iced's frame `target`, `set_scissor_rect(clip_bounds)` +
     `set_viewport(bounds)`, draw grid → wires → translucent lobe.
3. View side: `shader(&self.scene).width(Fill).height(Fill)`.

The primitive receives **iced's own shared `wgpu::Device`/`Queue`** (wgpu
0.19.4, re-exported at `iced::widget::shader::wgpu` — use the re-export, do
NOT add a direct wgpu dependency to nec-gui). Requires iced feature
`advanced` (iced's own `custom_shader` example enables `["debug", "image",
"advanced"]`; `wgpu` is a default feature).

### Threading model

- **UI thread**: iced event loop only. Never solves.
- **One-shot solves** (`Solve`, geometry rebuild, 3-D pattern): the existing
  `Task::perform(async move { blocking_fn() }, Message::…Complete)` pattern —
  already proven in `main.rs` to run off the UI thread on iced's background
  executor. Keep it.
- **Sweeps with progress**: `Task::run(iced::stream::channel(...), Message::SweepProgress)`
  — a worker (std::thread or the stream's async block) iterates frequency
  points and sends each `SweepPoint` as it lands; UI plot fills incrementally.
  Frequency points are embarrassingly parallel → `rayon::par_iter` over the
  frequency list inside the worker, sending results as they complete
  (out-of-order OK, plot sorts by freq).
- **Pattern grid + mesh gen**: `compute_radiation_pattern` cost is
  O(points × segments); parallelize by chunking the θ/φ point list across
  rayon and concatenating. Mesh triangulation (~5 k tris) is trivial, done on
  the same worker. `rayon` becomes a direct nec-gui dependency (already in the
  workspace).
- **GPU compute (`nec_accel`)**: untouched; a later optional increment can
  route the sweep worker through `HallenFrGpuKernel`. Render and compute
  devices are independent (§3.5).

### Solver-crate reuse

- Geometry preview: `nec_solver::build_geometry(&deck)` alone — cheap enough
  to run on every valid editor apply for instant visual feedback, no solve.
- Currents: `solve.rs::solve_for_currents` already returns
  `(Vec<Segment>, Vec<Complex64>, freq_hz, GroundModel)` — exactly what the
  viewport needs; it just must be *exposed* (today it is private) and its
  result carried into `SceneGeometry`.
- 3-D pattern: same function's output feeds `compute_radiation_pattern` on a
  full θ×φ grid (§3.4).

---

## 3. Technical feasibility findings

### 3.1 iced 0.13 custom-wgpu story — CONFIRMED, no fallback needed

`iced::widget::shader` **exists in 0.13.1** (verified on docs.rs):
`Shader` widget, `Program` trait, `Primitive` trait, `Storage`, `Event`,
`Viewport`, plus a full `wgpu` re-export. Exact signatures (verified):

```rust
pub trait Program<Message> {
    type State: Default + 'static;
    type Primitive: Primitive + 'static;
    fn draw(&self, state: &Self::State, cursor: Cursor, bounds: Rectangle) -> Self::Primitive; // required
    fn update(&self, state: &mut Self::State, event: Event, bounds: Rectangle,
              cursor: Cursor, shell: &mut Shell<'_, Message>) -> (Status, Option<Message>);    // provided
    fn mouse_interaction(&self, state: &Self::State, bounds: Rectangle, cursor: Cursor) -> Interaction;
}
pub trait Primitive: Debug + Send + Sync + 'static {
    fn prepare(&self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat,
               storage: &mut Storage, bounds: &Rectangle, viewport: &Viewport);
    fn render(&self, encoder: &mut wgpu::CommandEncoder, storage: &Storage,
              target: &wgpu::TextureView, clip_bounds: &Rectangle<u32>);
}
```

Constraints discovered:

- The widget **shares iced's device/queue/encoder/target** — you draw into
  iced's frame between its own layers. No swapchain of your own.
- **No depth buffer is provided** — create your own `Depth32Float` texture in
  `Storage`, recreate on viewport resize (iced's own `custom_shader` example
  does exactly this).
- The primitive is rebuilt every `draw()`; keep it a cheap handle bundle
  (camera matrix + `Arc<MeshData>` + revision counter), never raw geometry
  rebuilds per frame.
- Continuous redraw during drags happens naturally: each produced `Message`
  triggers `update` → `view` → new primitive. `Event::RedrawRequested` exists
  for animation, not needed for camera-driven redraws.
- WGSL targets **wgpu 0.19 / naga 0.19** — avoid post-0.19 WGSL niceties.

**Fallback (not needed, recorded for completeness)**: render offscreen with any
wgpu version → copy to `image::Handle` per frame. Strictly worse (CPU
round-trip per frame); only relevant if we ever abandoned the shader widget.

### 3.2 Camera + interaction

Orbit (arcball-lite) camera, pure math in `viewport/camera.rs`:

```rust
pub struct OrbitCamera { pub target: Vec3, pub distance: f32,
                         pub yaw: f32, pub pitch: f32, pub fov_y: f32 }
impl OrbitCamera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 { /* glam look_at_rh * perspective_rh */ }
    pub fn orbit(&mut self, dx: f32, dy: f32);   // yaw += dx·k; pitch clamp ±89°
    pub fn zoom(&mut self, steps: f32);          // distance *= 0.9^steps, clamped
    pub fn pan(&mut self, dx: f32, dy: f32);     // target moves in camera plane
    pub fn fit(&mut self, bbox: (Vec3, Vec3));   // frame geometry on load
}
```

Event flow: `Program::update` receives `shader::Event::Mouse(...)` only while
the cursor is over the widget bounds. Left-drag → `Orbit`, wheel → `Zoom`,
middle-drag (or shift+left) → `Pan`; return
`(Status::Captured, Some(Message::Viewport(...)))`. NEC is z-up: build the
view matrix with `Vec3::Z` up so antennas stand upright.

### 3.3 Wire + current rendering

- **Phase A (lines)**: vertex = `{ position: [f32;3], color: [f32;4] }`, two
  vertices per segment, `PrimitiveTopology::LineList`. 10 k segments = 20 k
  vertices — trivial. Line width is 1 px in wgpu; acceptable for A.
- **Phase B (instanced cylinders)**: one canonical 8-side cylinder mesh +
  per-segment instance `{ mat4 transform, vec4 color }` built from
  `Segment{start,end,radius}` (visual radius = max(physical, k·scene_size) so
  thin wires stay visible). Instanced draw, picking-friendly later.
- **Current coloring**: per-segment `|I|` normalized to `[0,1]` (linear over
  mA, matching `current_display_bars`), mapped through a viridis LUT in
  `mesh.rs` (pure fn → unit-testable: known inputs → exact RGBA). Recolor =
  vertex-buffer rewrite, no pipeline change. Phase toggle: optionally color by
  `arg(I)` later — the `Vec<Complex64>` is already in hand.

### 3.4 Pattern lobe mesh

Input: full-sphere grid from `compute_radiation_pattern` — θ ∈ [0°,180°] and
φ ∈ [0°,360°] at 5° (37×73 = 2 701 points; last φ column duplicates the first
for a closed seam).

- Radius: `r(θ,φ) = ((gain_dbi − g_min) / (g_max − g_min)).clamp(0,1)` with
  `g_min = max(actual min, g_max − 40 dB)` floor (and the −999.99 sentinel
  clamped first — same handling as `pattern_display_rows`).
- Vertex position = `r · r̂(θ,φ)` scaled so the unit lobe spans ~1.2× the
  geometry bounding radius; vertex color = colormap(gain_dbi).
- Indices: 36×72 quads → two triangles each (~5.2 k tris). Draw as a second
  pipeline with alpha blending (α ≈ 0.55), depth **test on / write off** so
  wires stay crisp inside the translucent lobe; render lobe after wires.
- `mesh.rs::build_lobe(grid) -> MeshData` is pure → headless unit tests
  (vertex count, isotropic-pattern ⇒ sphere, dipole ⇒ null on axis).

### 3.5 Coexistence with `nec_accel` — separate devices, by construction

The lock file already links **two wgpu major versions**: iced_wgpu 0.13.5 →
wgpu **0.19.4** (render) and nec_accel → wgpu **29.0.3** (compute,
`new_without_display_handle`, no surface). They are distinct crates to cargo;
they cannot share `Device`/`Buffer` types at all, and don't need to: solver
outputs cross as plain `Vec<f64>/Complex64`, and render meshes are tiny.

**Recommendation**: keep them fully separate. Do not attempt device sharing.
Costs: some binary bloat and two GPU contexts at runtime — acceptable.
Convergence (single wgpu) is possible only by bumping iced to 0.14+ (newer
wgpu); track as an optional follow-up (Phase 9 note), not a blocker.

### 3.6 Dependencies (workspace-level unless noted)

| Dep | Purpose | Status |
|:----|:--------|:-------|
| `iced 0.13` features `+= ["advanced", "canvas", "tokio"→no (default executor fine)]` | shader widget (`advanced`), 2-D plots (`canvas`) | edit `apps/nec-gui/Cargo.toml` |
| `glam` (feature `bytemuck`) | camera/mesh math, GPU-layout types | **new** workspace dep |
| `bytemuck` | vertex/uniform casting | already in workspace |
| `rayon` | sweep + pattern-grid parallelism | already in workspace |
| `rfd` | native file-open dialog | **new**, Phase 9 only |
| `wgpu` (direct) | — | **not added**; use `iced::widget::shader::wgpu` re-export |

---

## 4. Phased increments

Each phase = one branch → PR → squash-merge, with `cargo fmt`/clippy/tests
green and the doc ledger updated. Headless tests extend
`apps/nec-gui/tests/gui_smoke.rs` (+ new `viewport_math.rs` for pure mesh/camera
fns); GPU-visual gates are a screenshot in the PR description (CI stays
headless).

| # | ID | Goal | Effort |
|:--|:---|:-----|:-----|
| 0 | GUI-CHK-001 | Shader-widget spike: hello-triangle in a Viewport tab — **✅ code + headless gates done (2026-07-10)**; visual gates pending a display | 1–2 d |
| 1 | GUI-CHK-002 | Wire geometry rendered in 3-D (+ axes, ground grid) — **✅ code + headless gates done (2026-07-10)**; visual pending a display | 2–3 d |
| 2 | GUI-CHK-003 | Orbit / zoom / pan camera — **✅ code + headless gates done (2026-07-10)**; visual pending a display | 2 d |
| 3 | GUI-CHK-004 | Currents painted on wires + colormap legend — **✅ code + headless gates done (2026-07-10)**; visual pending a display | 1–2 d |
| 4 | GUI-CHK-005 | 3-D pattern lobe overlay (full-sphere) — **✅ code + headless gates done (2026-07-10)**; visual pending a display | 3 d |
| 5 | GUI-CHK-006 | pane_grid layout shell; tabs dissolve into panes — **✅ code + headless gates done (2026-07-10)**; visual pending a display | 2–3 d |
| 6 | GUI-CHK-007 | Deck writer + GW wire table editor (live preview) | 3–4 d |
| 7 | GUI-CHK-008 | EX / GN / LD / FR editors + Apply-and-Solve | 3 d |
| 8 | GUI-CHK-009 | Streaming sweep, SWR/|Z| canvas plot, frequency slider | 3 d |
| 9 | GUI-CHK-010 | Polish: file dialogs, view options, fit/reset, project save | 2–3 d |

Total ≈ 22–28 dev-days. 0→4 de-risk the GPU path first; 5→8 build the
workbench; 9 polishes. Phases 3/4 are swappable; 6 blocks 7.

### Phase 0 — GUI-CHK-001: shader spike (full spec in §6)

**Gate**: triangle visible + resizes in a new "Viewport" tab; all existing
headless tests pass unmodified; screenshot in PR.

### Phase 1 — GUI-CHK-002: wire geometry in 3-D

Files: `viewport/{mesh.rs,pipeline.rs,shaders/wires.wgsl,shaders/grid.wgsl}`,
`solve.rs` (`load_geometry(path, vars) -> Result<SceneGeometry, String>` =
parse + `build_geometry`, no solve), `app_state.rs`
(`Message::GeometryLoaded`, `SceneGeometry` in state, `scene_rev`), `main.rs`
(Load-geometry button in Viewport tab). Fixed isometric camera; xyz axis
triad + z=0 grid (shown when `GroundModel != FreeSpace`, from
`ground_model_from_deck`); camera `fit()` to bbox on load.
**Gates**: (a) headless: `mesh.rs::wires_to_vertices` on a 3-wire deck →
2×segs vertices, correct endpoints/bbox; (b) visual: `examples/` dipole and a
multi-wire yagi deck screenshot matching the deck coordinates; (c) existing
tests green.

### Phase 2 — GUI-CHK-003: camera interaction

Files: `viewport/camera.rs`, `viewport/mod.rs` (`Program::update` mouse
handling), `app_state.rs` (`Message::Viewport(ViewportMsg)` applied in
`apply()`).
**Gates**: (a) headless: orbit/zoom/pan/fit unit tests (pitch clamp, zoom
bounds, pan in camera plane, view_proj round-trips a known point); (b)
headless: `AppState::apply(Viewport(...))` mutates camera; (c) visual: drag
orbits, wheel zooms about target, middle-drag pans, Reset restores fit.

### Phase 3 — GUI-CHK-004: currents on the geometry

Files: `solve.rs` (expose `solve_for_currents`-equivalent returning segments +
`Vec<Complex64>`), `viewport/mesh.rs` (colormap + recolor), `main.rs`
(☑ Currents toggle, legend strip with min/max mA labels).
**Gates**: (a) headless: colormap LUT exact-value tests; recolor of a known
current vector → expected per-vertex RGBA; (b) visual: λ/2 dipole shows
bright feedpoint center, dark tips (textbook cosine taper); (c) toggle returns
to uniform color.

### Phase 4 — GUI-CHK-005: 3-D pattern lobe

Files: `solve.rs` (`pattern_grid_deck_str` — full sphere, rayon-chunked
`compute_radiation_pattern`), `viewport/{mesh.rs::build_lobe,
shaders/pattern.wgsl, pipeline.rs}` (blended pipeline, depth write off),
`app_state.rs` (`Pattern3dComplete`, ☑ Pattern toggle).
**Gates**: (a) headless: `build_lobe` vertex/index counts; isotropic grid ⇒
all radii equal; dipole grid ⇒ axis nulls; sentinel −999.99 handled; (b)
visual: dipole donut around the wire axis; with `GN 1` deck, hemisphere only;
(c) UI stays responsive during grid compute (status text "Computing pattern…").

### Phase 5 — GUI-CHK-006: workbench layout

Files: `main.rs` (toolbar + `pane_grid` of sidebar/viewport/results +
bottom strip), `panels/results.rs`, `app_state.rs` (pane state,
`PaneResized`). Existing tab content relocates: Solve→results panel,
Pattern/Currents→viewport toggles + results, Sweep→results (plot lands in
Phase 8).
**Gates**: (a) headless: all prior state tests still pass (message surface
unchanged for solve paths); (b) visual: three resizable panes per §1 mockup;
(c) `gui_smoke.rs` extended for pane-resize message.

### Phase 6 — GUI-CHK-007: deck writer + wire editor

Files: `deck_write.rs` (**new**: `Card → String` NEC line serializer —
none exists in the workspace; round-trip via `nec_parser::parse` is the test
oracle), `model_doc.rs` (editable `WireRow { tag, segments, x1..z2, radius }`
string fields, validation, dirty flag), `editors/wires.rs` (scrollable row
table: text_inputs + Add/Delete row), live `build_geometry` preview on every
valid edit (no solve), Save writes the deck file.
**Gates**: (a) headless: parse→write→parse round-trip equality on the example
corpus decks (GW/GE/FR/EX/GN/LD subset); (b) headless: `model_doc` validation
(bad float rejected, radius > 0, segments ≥ 1); (c) visual: editing a z
coordinate moves the wire in the viewport immediately; added wire appears.

### Phase 7 — GUI-CHK-008: EX / GN / LD / FR editors

Files: `editors/{excitation.rs,ground.rs,loads.rs,frequency.rs}`,
`model_doc.rs` rows mirroring `ExCard`/`GnCard`/`LdCard`/`FrCard` fields
(pick_list for GN type −1/0/1/2 and LD type 0–5; tag/segment pick_list fed
from geometry). `[Apply + Solve]` rebuilds the deck string and reuses the
existing solve pipeline.
**Gates**: (a) headless: each editor row ↔ card round-trip; invalid
tag/segment ref rejected with message; (b) visual: moving the EX feedpoint
changes viewport current hotspot after re-solve; switching GN free-space→PEC
adds the grid and changes Z; (c) round-trip corpus gate from Phase 6 extended.

### Phase 8 — GUI-CHK-009: streaming sweep + plots + frequency slider

Files: `solve.rs` (sweep worker: rayon over freq points → `iced::stream::channel`),
`app_state.rs` (`SweepProgress` accumulates sorted points; `FreqSliderMoved`),
`panels/{sweep_plot.rs,pattern2d.rs}` (iced `canvas`: SWR+|Z| vs f with
cursor readout; 2-D polar slice replacing the text bars), `main.rs` (slider
enabled once sweep data exists; drag → nearest cached point, release →
re-solve currents/pattern at that f).
**Gates**: (a) headless: progress accumulation keeps points sorted; slider
snapping logic unit-tested; (b) visual: plot fills point-by-point during a
50-point sweep while the viewport stays interactive; slider scrubs the
current coloring across resonance; (c) sweep of the resonance-search example
reproduces the CLI sweep values (same numbers as `sweep_deck_str`).

### Phase 9 — GUI-CHK-010: polish

`rfd` file dialogs (Open/Save), view menu (background, colormap choice,
lobe α, line/cylinder wire mode — Phase B instancing lands here if not
earlier), keyboard shortcuts, `nec_project` save/load of last session
(deck path + camera + sweep range), README/docs screenshots.
**Gates**: (a) headless state tests for new options; (b) visual walkthrough
recorded in the PR; (c) `docs/gui-guide.md` (new user doc) added with
screenshots; changelog entry.

---

## 5. Risks + mitigations

| Risk | Likelihood | Impact | Mitigation |
|:-----|:-----------|:-------|:-----------|
| iced 0.13 shader widget quirks (docs are thin; Storage/depth handling is by-example) | Med | High | Phase 0 spike is *only* this; iced 0.13 `custom_shader` example is the reference implementation; fallback = offscreen-render-to-`image::Handle` (§3.1) |
| Two wgpu versions (0.19 render + 29 compute) — binary bloat, doubled GPU context | Certain (by design) | Low | Accepted; no sharing attempted; optional later iced-0.14 bump converges them |
| wgpu 0.19 WGSL/naga is old — newer shader syntax fails | Med | Low | Shaders kept trivial (vertex color, one uniform); no post-0.19 features |
| No depth buffer from iced → wrong occlusion | Certain | Med | Own `Depth32Float` texture in `Storage`, recreated on resize (Phase 0 gate includes resize) |
| GUI CI is headless — 3-D output untestable in CI | Certain | Med | All mesh/camera/colormap math pure + unit-tested; visual gates = PR screenshots (house pattern already: `gui_smoke.rs`) |
| Blocking solves starve iced's executor pool during long sweeps | Med | Med | Sweep worker = dedicated thread feeding `stream::channel`; rayon pool does the math |
| Deck writer round-trip loses card fidelity (comments, unusual spacing) | Med | Med | Round-trip = parse-equality not byte-equality; corpus decks as oracle; comments preserved as raw lines in `model_doc` |
| `text_input` grid editor ergonomics poor for large decks | Med | Low | Scrollable virtualized rows; editor targets tens of wires (typical), not thousands |
| iced 0.13 EOL vs 0.14 migration later | Med | Med | Viewport isolated behind `viewport/` + the `shader::Program` seam; camera/mesh are iced-free |

**New dependencies (complete list)**: `glam` (workspace, `bytemuck` feature),
`rfd` (Phase 9), iced features `advanced` + `canvas` on `apps/nec-gui`;
`bytemuck`/`rayon` promoted from workspace into nec-gui deps. No direct `wgpu`.

---

## 6. Phase 0 spec — GUI-CHK-001 "hello triangle in the shader widget"

**Branch**: `feat/gui-chk-001-shader-spike`. Goal: prove the §3.1 mechanism
end-to-end in this repo with zero solver coupling.

1. **`apps/nec-gui/Cargo.toml`**: `iced = { version = "0.13", features = ["advanced"] }`;
   add `glam = { workspace = true }`, `bytemuck = { workspace = true }`; add
   `glam = { version = "0.29", features = ["bytemuck"] }` to the workspace
   `[workspace.dependencies]`.
2. **`src/viewport/mod.rs`** — `pub struct Scene;` implementing
   `shader::Program<Message>` with `type State = ()`, `type Primitive =
   TrianglePrimitive`, `draw()` returning `TrianglePrimitive { t: 0.0 }`.
3. **`src/viewport/primitive.rs`** — `TrianglePrimitive` implementing
   `shader::Primitive`:
   - `prepare`: `storage.get::<Pipeline>()` miss → build from
     `shaders/triangle.wgsl` (hard-coded 3 clip-space vertices in the vertex
     shader, no vertex buffer) **and** a `Depth32Float` texture sized to
     `viewport.physical_size()`; store both. On size change, recreate depth.
   - `render`: render pass on `target` with `LoadOp::Load` (do NOT clear —
     iced's UI is already there), depth attachment cleared to 1.0,
     `set_scissor_rect(clip_bounds)`, `set_viewport(bounds as physical px)`,
     `draw(0..3, 0..1)`.
   - All wgpu types via `iced::widget::shader::wgpu`.
4. **`src/app_state.rs`**: add `ActiveTab::Viewport` (5th tab). No other state.
5. **`src/main.rs`**: tab button + `ActiveTab::Viewport =>
   container(shader(&Scene).width(Length::Fill).height(Length::Fill))`.
6. **Tests**: `gui_smoke.rs` gains `viewport_tab_selectable` (headless message
   test). No GPU in CI.

**Gates**:

- G1: `cargo run -p nec-gui` → Viewport tab shows a colored triangle inside
  the tab area (not full-window: proves scissor/viewport math), UI widgets
  around it still render (proves LoadOp::Load correct).
- G2: window resize + pane area change do not panic and keep the triangle
  proportioned (proves depth-texture recreation path).
- G3: `cargo test -p nec-gui` green headlessly; fmt/clippy clean.
- G4: PR includes a screenshot; this doc's phase table gets a ✅ status note.

**Estimated effort**: 1–2 days. Everything after Phase 0 is incremental
replacement of the triangle with real content — the integration risk dies
here.

**Status (2026-07-10)**: implemented on `feat/gui-chk-001-shader-spike`. New
`apps/nec-gui/src/viewport/{mod.rs,primitive.rs,shaders/triangle.wgsl}`,
`ActiveTab::Viewport`, `shader(Scene)` in a new "3D View" tab. Integration
findings confirmed against the real toolchain: iced 0.13.1's re-exported wgpu is
**0.19.4**, which has **no `compilation_options`** on `VertexState`/`FragmentState`
(removed — a live instance of the §3.1 "old naga/wgpu" note); the private
`Depth32Float` texture + `LoadOp::Load` + scissor/viewport-to-`clip_bounds`
approach compiles. **G3 met**: `cargo test -p nec-gui` (48 headless tests incl.
`viewport_tab_selectable`) green, clippy `-D warnings` + fmt clean. **G1/G2/G4
(pixels/resize/screenshot) require a display** and are verified by running
`cargo run -p nec-gui` on a workstation — not reproducible in the headless build
environment, so they are handed off rather than self-certified.
