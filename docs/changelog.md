---
project: fnec-rust
doc: docs/changelog.md
status: living
last_updated: 2026-08-25
---

# Changelog

All notable changes to this project are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Sections
from 0.13.0 and earlier predate the Keep a Changelog headings and are left as written.

## [Unreleased]

### Added

- **The GUI has a solver picker** (FND-007). Decks with a T/Y junction, a closed
  loop, or currents near lossy ground could only be solved correctly from the
  CLI; the GUI warned about them and told the user to leave. It now offers
  **Hallén** and **MPIE**, and the choice reaches every tab.

  The branch is in the one shared solve step, so Solve, Sweep, Currents and
  Pattern all follow the picker together. A picker that changed only the tab in
  front of you would be the FND-038 defect one solver over.

  It reproduces the CLI exactly: the pinned free-space dipole (74.437414 +
  j41.753720) and the degree-3 Y-junction (63.673674 - j322.199211) — the case
  the MPIE exists for, where Hallén returns R≈8 garbage.

  **Caveats now know which solver is running.** A T/Y deck on the MPIE no longer
  carries the Hallén topology caveat, which was not merely redundant there but
  false — and whose remedy would have recommended the solver already running.
  Nothing in the GUI quotes a CLI flag any more: `validate::SolverContext` carries
  the remedy, so each frontend phrases it for its own users. The same change
  retired the CLI's private per-solver negative-resistance arm into the shared
  producer.

  A deck the MPIE cannot represent — `LD`, `TL`, `NT` — is **refused** on that
  solver rather than solved with the card ignored, and a sweep refuses it when the
  job is prepared rather than after queueing every point.

  Switching solver **discards every solved view** — impedance, sweep, pattern,
  currents, the 3-D overlay and the deck caveats — and results from a solve still
  in flight when you switch no longer repopulate them. Leaving any of them up
  would put one solver's impedance beside another's pattern with nothing saying
  so, which is the disagreement the picker exists to prevent. The choice is
  written to the session file, like the chart-metric picker beside it.

  One CLI string changed wording: the junctionless negative-resistance cause now
  reads "re-run with `--solver mpie` to cross-check" instead of "cross-check with
  `--solver mpie`", because the remedy is supplied by the caller now. The topology
  remedy, the MPIE "solver defect" cause, and the `LD`/`TL`/`NT` refusals are
  byte-identical.

  Only the two production solvers are offered. The experimental pulse, continuity
  and sinusoidal modes are known-inaccurate for thin-wire antennas, and a picker
  is an invitation to use what it lists. `fnec_py` still has no solver choice —
  recorded as FND-055 rather than left as an oversight.

- **`scripts/check-all.sh`** — the full local gate over **both** cargo trees
  (FND-024). `bindings/fnec_py` sits outside the workspace, so `cargo fmt --all
  --check` at the root exits 0 on an unformatted bindings crate and CI fails on it
  instead; that happened, and cost a red build on a two-line import wrap after
  every local check had passed. The fmt step runs from `bindings/fnec_py`, which
  sees both trees because the workspace crates are path dependencies there.

- **The GUI and the Python bindings solve current-source decks** (FND-045). They
  used to decline an `EX 4` deck and say "use the fnec CLI" — but the machinery
  was never missing, only unwired: `nec_solver` has exported
  `solve_hallen_current_source` all along, and the routing around it lived in the
  CLI. `corpus/dipole-ex4-freesp-51seg.nec` now gives **74.227929 + j13.896926**
  in the GUI, identical to the CLI, and the tests assert the CLI's corpus value so
  the frontends cannot drift.

  It was **not** safe to share this until two defects were fixed first: a
  collinear split delivered half the requested current (FND-048), and a deck
  carrying both drive kinds was answered rather than refused (FND-036). Promoting
  the glue before those would have turned one wrong frontend into four.

  The branch is at the *solve* step, not the pricing step — a current-driven
  deck's excitation vector is all zeros, so `V/I` has nothing to divide. All three
  GUI paths go through one shared step, because solving on the Solve tab while the
  sweep, currents and pattern views refused would be the same one-tab-over defect
  this arc keeps turning up.

  The remote worker still refuses them: a scope choice rather than a technical
  one, recorded as FND-051 so the remaining difference is deliberate and visible.

- **A release-tag minting workflow** (FND-046) — a button, not a trigger.

  The obvious design is "push to main, version has no tag, mint it". It was
  proposed, and this repository's own history refutes it: at `2f51d63`, the
  v0.15.0 release-PR merge that push-minting would have tagged, `pyproject.toml`
  says 0.4.0 against a 0.6.0 crate — the FND-044 wheel-label defect, live. The
  real tag sits two commits later at `1354fda`, after the defect was found by
  hand-building the wheel. Auto-minting would have created the tag that then had
  to be deleted, and a "never move a tag" rule would have set the automation
  against the person fixing it.

  The pause between *merged* and *tagged* is where the only recent release defect
  was caught. `workflow_dispatch` keeps it and removes the other half of the risk:
  a human decides that a release is ready, and the machine verifies the commit and
  refuses to tag one that does not deserve it. Demonstrated against real history —
  the pre-mint check exits 1 at `2f51d63` and 0 at `1354fda`.

  Verification runs **before** the tag is pushed, never after: a red job standing
  beside a published bad tag is worse than no automation, because the workflow may
  not delete it. `contents: write` is scoped to the one job, which installs no
  toolchain and builds nothing — a job whose token can rewrite release history
  should not also run build scripts from the dependency tree.

  Dropping the unattended repair mode reopened the deleted-tag blind spot, so
  `check-release-tags.py`'s in-flight exemption now **expires with age** and CI
  gained a weekly schedule. A push-triggered check cannot notice that nothing
  happened, and "a release was merged and never tagged" is exactly nothing
  happening.

  Hardened before its first run, which is the only cheap moment. The tag message
  is written with `--cleanup=verbatim`, because git's default for `-F` deletes
  every line beginning with `#` — and a Keep-a-Changelog body is structured by
  exactly those. Measured on the 0.15.0 section: four `###` headings in the file,
  four preserved with the flag, **zero without it**, which would have merged four
  categorised lists into one stream where a breaking change and a bugfix are
  indistinguishable. The job also gains `checks: read` — job-level `permissions:`
  replaces the map rather than extending it, so the check-runs gate was relying on
  that endpoint answering unauthenticated, which is true only while the repository
  is public. Plus `--paginate`, so a failing check past the first page of thirty
  cannot pass unseen, and every resolved value now reaches the shell through
  `env:` instead of `${{ }}` interpolation.

- **A release-tag integrity check** (FND-043). Five versions — 0.4.0, 0.5.0,
  0.6.0, 0.8.0 and 0.9.0 — were released without tags, so they have changelog
  sections and no ref to check out, compare against or link. And v0.15.0 was
  first tagged at a commit whose tree still said `fnec_py` 0.4.0, caught by hand
  minutes before publishing by building the wheel and reading its filename.

  `scripts/check-release-tags.py` asserts both directions: every released version
  has a tag, and every tag names the version its own tree declares. The newest
  section is exempt only while it matches the workspace version at `HEAD` — a
  release PR must not fail its own gate, but a stray section is not excused.

  It runs in the docs job and on `push: tags: 'v*'`, which is the one moment a
  bad tag can be caught before anyone acts on it. A push-triggered check
  otherwise fires only when something *else* pushes, so a release merged and
  never tagged would stay green until the next release PR — possibly months away.

  The five are grandfathered as a frozen set rather than a `BEFORE` cutoff,
  because the gaps interleave with present tags and a cutoff would stop guarding
  v0.3.0 and v0.7.0 against deletion — not hypothetical, since v0.15.0 was
  deleted and re-pushed during its own release. v0.14.0's wheel-label defect is
  exempted separately and by name: that tag is published and immutable, and
  re-pointing it to satisfy a checker would be worse than the defect it records.

  Ships with `scripts/test-check-release-tags.py`, the first committed self-test
  among these checkers — the gates that let both findings through were passing
  honestly while looking in the wrong place. It builds throwaway git repositories
  to exercise the decisions themselves, after review showed three ways to hollow
  the checker out with every pure-function case still green.

  One limit is recorded rather than glossed: a *deleted* tag is invisible for the
  newest release, because "newest section, no tag" cannot be distinguished from
  "release in flight" from inside the repository — and the newest is the only tag
  anyone has actually deleted. Closing that needs the minting workflow (FND-046),
  not a better detector.

### Changed

- **The MPIE solve is a library capability, not CLI glue** (FND-007 groundwork,
  FND-037, FND-052). `solve_mpie_session` moved from the CLI into
  `nec_solver::mpie_session`, so a second frontend can offer the MPIE without
  reimplementing which excitation feeds the geometry and which decks it must
  refuse.

  The refusals moved **with** it, into the solve rather than beside it. The MPIE's
  triangle basis has nowhere to stamp an `LD`, `TL` or `NT`, and its delta-gap
  feed cannot represent an incident field, so a deck carrying either would be
  solved with the offending card silently ignored. Those checks sat in a
  CLI-private function; a caller that skipped them got no answer, it got a wrong
  one. `MpieUnsupported` is typed rather than a string, because a CLI naming a
  flag, a GUI naming its picker and a Python exception are three audiences for one
  decision.

  The mixed-radius caveat moved too (FND-052) — the MPIE solves the whole geometry
  with the first segment's radius, and that warning was an `eprintln!` in the CLI.
  A shared solve whose caveat stayed behind is how one frontend ends up quietly
  approximating.

  Feedpoint selection is now `first_delta_gap_feedpoint`, closing FND-037 at the
  move: the old `!is_plane_wave()` test admitted a type-4 current source and any
  unrecognised `EX` type, and was safe only because callers happened to reject
  those first. A shared function may not depend on what its callers checked.

  No answer changed: the free-space dipole (74.437414 + j41.753720), the degree-3
  Y-junction (63.673674 - j322.199211), a dipole over `GN 2` (73.857642 +
  j30.548668), an `EX 5` dipole and an apex-fed inverted-V are all digit-identical
  to before the move. The first three are now pinned as acceptance values so any
  frontend adopting the MPIE must reproduce them.

  **Two messages did change wording**, both because a shared string cannot name a
  CLI flag: the mixed-radius caveat now opens "the MPIE solver models a single
  wire radius" rather than "`--solver mpie` models…", and the no-interior-node
  error drops a duplicated subject. The `LD`/`TL`/`NT` refusals and the
  plane-wave/current-source refusals are byte-identical.

### Fixed

- **A refused deck still reports the caveats it earned** (FND-059). A deck can be
  both flawed *and* refused — an unrecognised card **and** no driven feedpoint —
  and `TaskResult::Error` had nowhere to carry the first. The reader was told the
  solve stopped and never that a line had been ignored on the way there, which is
  often the reason it stopped.

  The field is `#[serde(default)]`, additive both ways like the one FND-026 added
  to `Ok`: an older worker sends none and this reads empty, and an older
  controller ignores a newer worker's. A new `ErrorCode` variant would not have
  been — `ErrorCode` has no `#[serde(other)]`, so an unknown one fails the whole
  result line, which the pool reads as a dead worker and evicts.

  The controller had been destructuring the error with `..`, which is exactly how
  the `Ok` arm's warnings went unread for a release, so the decision of what to
  print is now a named function a test can reach rather than a line inside a loop.
  A task refused *before* anything is parsed reports no caveats, and that is
  asserted too — it must not invent them for a deck that never existed.

- **The remote worker stops losing information the local path keeps** (FND-049,
  FND-041). Two separate leaks, same class.

  A deck that **parsed cleanly** crossed the wire as `parse_error`. The
  `SolveError` → `ErrorCode` mapping ended in a catch-all, so anything it had not
  named inherited `ParseError` — including `NoFeedpoint`, which a plane-wave
  (`EX 1`) receive deck returns and which the local CLI solves happily. The reader
  went hunting for a syntax mistake that was not there. The mapping is exhaustive
  now, so a new `SolveError` forces a decision at compile time rather than
  silently inheriting the wrong code; a genuine syntax error still earns
  `ParseError`.

  Separately, the worker parsed the deck and dropped its caveats on the next line
  (`let deck = parse_result.deck;`), so an ignored card was never reported. That
  was masked for the CLI, which parses the identical bytes locally and prints its
  own — invisible for exactly one caller and total for every other, including
  anything driving the public `run_worker_stdio`.

  Sending them exposed a second-order defect in the same change: the CLI already
  prints local parse warnings **once**, so an M-point distributed sweep would have
  printed the same caveat M+1 times where a local run prints 1. The controller now
  prints each distinct worker line once, keyed on the rendered text so the same
  message from *different* workers is still shown separately — which is exactly
  what a mixed-version pool needs.

- **A degenerate frequency is refused instead of answered** (FND-056, FND-030).
  Nothing validated `FR` at all, and the results were not merely wrong but
  confidently wrong. Measured on a 21-segment dipole before the fix:

  | deck | reported | warnings |
  |:---|:---|:---|
  | `FR ... 0.0` | `Z = 1.000000 + j0.000000` | none, exit 0 |
  | `FR ... -14.2` | `67.161824 + j32.275596` | none, exit 0 |
  | `FR 0 5 0 0 10.0 -3.0` | solves and prints `FREQ_MHZ -2.000000` | none, exit 0 |

  At zero the current is driven to zero, so `Z = V/I` takes its zero-current
  branch and prints the `EX` **source voltage** back as an impedance. At −14.2 MHz
  the answer is the exact complex **conjugate** of the +14.2 MHz one, so a typo'd
  minus sign flips the reactance between capacitive and inductive and reports it
  as fact.

  `validate::frequency_error` is folded into the shared `pre_solve_error`
  aggregate, so all four frontends refuse together. It checks the **generated**
  frequency list rather than the card's first field: a descending sweep passes any
  start-value test and still walks into negative frequency.

  Finiteness is checked in **hertz**, not megahertz. Every frontend multiplies by
  1e6, so a *finite* field near the top of the range becomes an infinite
  frequency: `FR 0 1 0 0 1e303 0` — every field finite, so the parser passes it —
  produced `FREQ_MHZ inf`, `NaN` currents and `Z = 1.000000 + j0.000000`, both
  defects at once from a deck the first version of this check accepted.

  The check takes the **extremes** of each `FR` card rather than expanding it.
  `steps` is a `u32`, and this runs inside `pre_solve_error` — which the GUI calls
  on every Apply+Solve and the worker on every task — so collecting the list would
  have let two integers stall those processes from inside validation. Both
  expansions are monotone, so the first and last elements decide the rest.

  The GUI's sweep range needed its own guard — it comes from the UI and never
  reaches an `FR` card, so a sweep from −5 MHz was invisible to the shared check.
  Its docstring had promised "positive float" all along while nothing checked it.

- **Non-finite deck fields are rejected at the parser** (FND-030). `f64::from_str`
  accepts `NaN`, `inf` and `-inf`, and a NEC deck is plain text, so
  `GW 1 21 0 0 NaN 0 0 5.0 0.001` printed a feedpoint row of `NaN NaN NaN NaN NaN
  NaN` at exit 0 with **no** warning. `NaN` defeats the downstream checks by being
  *unordered* rather than wrong: `current.norm() > 1e-60` is false for it, so the
  feedpoint takes its zero-current branch, and `z_re < 0.0` is false too, so no
  caveat fires. Rejecting in `parse_f64` closes the class for every card and every
  frontend at once — integer fields included, which go through `f64` because
  4nec2 emits "1.0" and so inherited the same acceptance: `NaN as u32` is 0 and
  `inf as u32` is `u32::MAX`, so `FR 0 NaN 0 0 14.2 0` parsed and solved as a
  one-step sweep.

  This closes the *input* routes only. A genuinely diverged CPU solve producing
  `NaN` is a separate route and stays open.

- **A deck driven by two kinds of source is now refused instead of answered
  wrongly** (FND-036). A dipole carrying both an `EX 0` and an `EX 4` reported
  **0.678 + j0.086 Ω** for the voltage feedpoint, where the same deck without the
  current source gives **74.243 + j13.900** — a hundredfold error at exit 0 with
  no warning. The current-source path replaces the right-hand side, so the delta
  gap was priced over currents its own drive never produced.

  Superposition would be the physically correct answer and is real solver work.
  Refusing is the honest interim: those numbers were never meaningful, so nothing
  is lost by declining to print them. The message names both offending cards,
  because "remove one" is unactionable if you cannot tell which two are fighting.

  All four frontends refuse it, through a new `validate::pre_solve_error`
  aggregate. That is deliberately not bolted into `geometry_error`: this is not a
  geometry problem, and a reader asking "why was my deck refused" would never
  think to look there.

- **A distributed run now says when it did not run where you asked** (FND-040).
  `--exec gpu --hosts` against a host with no adapter produced a CPU solve in
  silence. The worker had always reported which path it took; the controller
  discarded it, and wrote `ssh` into the benchmark record — naming the transport
  and hiding the execution path, so every distributed record read the same
  whether the work ran on a GPU or a CPU. It now warns, and records
  `ssh-{exec_used}`.

  It reports the **fact and not a cause**. An earlier draft added "that host has
  no usable adapter", which the controller cannot know and which is often false —
  the worker also declines the device for a deck under 16 segments, for anything
  but free-space or deferred ground, and for any live `LD`/`TL`/`NT` stamp.
  PH7-CHK-004's own acceptance evidence is that case: a loaded deck falling back
  on a GPU-capable node. Asserting an adapter fault there would have printed a
  wrong diagnosis on every worker of a healthy cluster, where the local CLI stays
  silent.

  Only on an explicit `--exec gpu`: without the flag the startup probe inspects
  the *controller's* adapter and can select GPU by itself, which says nothing
  about a remote host.

  `exec_used` defaults to `cpu` for a worker too old to send it — accurate rather
  than merely safe, since GPU execution and the field shipped together, so a
  worker that omits it has no GPU path to report.

- **A bad `EX` reference is no longer called a parse error** (FND-021). An `EX`
  naming a segment the geometry does not contain crossed the wire labelled
  `parse_error`, sending the reader to hunt for a syntax mistake in a deck that
  parsed cleanly. It is now `unsupported_config` — deliberately not a new
  `ErrorCode` variant, because that enum is serialised and adding one breaks an
  older controller's deserialisation outright, unlike the additive `warnings`
  field of FND-026.

- **A current source on a collinear-split wire delivered half its current**
  (FND-048). `solve_hallen_current_source` pins `I = 0` at the first and last
  segment of every entry in the endpoint list it is handed, and the
  current-source path passed the raw per-`GW` list — so a dipole written as two
  collinear cards carried a spurious zero at the join. With the source sitting on
  it, the solver was asked for `I = 1 A` and `I = 0` at the same segment, and
  least squares split the difference.

  | deck | Z | delivered current |
  |---|---|---|
  | 51-segment `EX 4` dipole | 74.228 + j13.897 Ω | 1.0 A |
  | identical geometry, two collinear `GW`s | **36.953 + j7.013 Ω** | **0.5 A** |
  | same split, `EX 0` control | 74.307 + j13.893 Ω | — |

  Exit 0, no warning. The voltage path has merged collinear joins since
  PH9-CHK-002; this one never did. Neither existing `EX 4` fixture is split, so
  nothing caught it.

  The new gate asserts the **delivered current** before the impedance. An
  impedance can be wrong for many reasons; a current source that does not deliver
  its own stated current has violated the boundary condition the user wrote down.

  Found while designing FND-045 — checking whether this glue was safe to share
  with the GUI and the bindings, which is exactly the sharing that would have
  turned one wrong frontend into four.

- **A current-source deck is now declined by name, not blamed on a missing card**
  (FND-038). `fnec-gui` and `fnec_py` fell through their feedpoint loop to
  "deck has no EX card" — false for a deck whose only excitation *is* an `EX`
  card, and it sent the reader hunting for something that was right there. A
  current source is a feedpoint; pricing one needs the solved port voltage, which
  only the CLI's Hallén path computes.

  The message now names the `EX` type, the tag and segment, the reason and the
  remedy:

  ```
  EX type 4 (current source) on tag 1 segment 26: a current-source feedpoint is
  priced from the solved port voltage, which this path does not compute; use the
  fnec CLI for this deck
  ```

  `validate::unpriceable_feedpoint_error` is the shared producer, with the remedy
  left to the caller because it genuinely differs — the distributed path says
  "run without `--hosts`". It absorbs the worker's inline copy in the same change,
  so the third frontend never grew a third wording.

  Two more of the same shape, found in review. The Currents and Pattern tabs had
  no such guard at all, so an `EX 4` deck rendered zero currents and a meaningless
  pattern while the Solve tab declined by name — FND-038's original defect, alive
  one tab over. And the remaining fallback still said "deck has no EX card" for a
  **plane-wave receive deck**, which has one; both frontends now use the worker's
  truthful "no driven feedpoint (EX voltage source) found in deck".

## [0.15.0] — 2026-08-25 — Every frontend tells the same truth

Nine changes closing sixteen findings, all of one shape: a check, a caveat or a
stamp that existed on one frontend and not the others, so the same deck got
different answers — or different silence — depending on how you asked. `fnec`,
`fnec-gui`, `fnec_py` and `fnec worker --stdio` are four shipped artifacts, and
this release is mostly about making that stop mattering.

### Added

- **A path inventory for cross-cutting concerns**
  (`docs/project/path-inventory.md`), enumerating every production execution path
  each concern must reach and what proves it does. Writing it immediately found
  **three gaps that "all three frontends" language had hidden**: the distributed
  `--hosts` path skips pre-solve validation at both ends (FND-013), the
  negative-resistance tripwire is CLI-only (FND-014), and the remote worker
  discards the load/TL builder warnings while `NT` cards are never stamped off the
  CLI path at all (FND-015). `scripts/check-path-inventory.py` fails CI if a cited
  test has been renamed away or a gap row links to no finding, so the inventory
  cannot decay into a coverage claim backed by nothing.

- **A findings ledger** (`docs/project/findings-ledger.md`). Discoveries used to
  scatter — one in a review document, one in a changelog entry, one in a design
  doc, one only in a spoken "still open: X, Y, Z". Nothing was lost, but that was
  down to the recital rather than to any structure. Every finding now gets an ID
  and a state, newest-first, and only `open` is non-terminal: `fixed` must cite the
  change, `deferred` must name an owner, `rejected` must give a rationale.
  `scripts/check-findings-ledger.py` enforces that shape in CI, so a deferred row
  with no owner fails the build instead of sitting there looking resolved. Seeded
  with 12 findings — 7 open, including the ones that previously existed only in
  conversation.

### Changed

- **GPU benchmark evidence refreshed, and the GPU-resident solve measured for the
  first time.** The stored crossover artifact predated #372/#373. Re-run on current
  code, the device caching shows up plainly: RP far-field production wall-clock
  drops from ~42 000 µs to ~1 000 µs (the ~23 ms per-call device init is gone), and
  the Z-fill kernel beats the CPU from N ≈ 32–64, by up to 290 ×.

  The harness gained a dense-solve section, which had never been measured — the
  feature was accepted on accuracy alone. **The GPU-resident solve is slower than
  the CPU at every size tested (0.04 × at N=32 rising to 0.48 × at N=256) and has no
  crossover**; at N=512 the #373 residual gate declines it outright. The cause is
  structural: the LU dispatches a *single workgroup*, so it runs on one compute
  unit while the other kernels spread across the device — more GPU hardware cannot
  fix it. This is what makes `--exec gpu` slower end to end despite two kernels
  that win by two orders of magnitude. Recorded in
  `docs/ph7-chk-003-gpu-resident-solve.md` § Performance.

  Two claims corrected while checking: that document's results were headed "real
  discrete GPU" when the adapter is `IntegratedGpu`, and its accuracy conclusion
  was stated without the size boundary it holds at.

### Fixed

- **The `fnec_py` wheel was labelled with the wrong version, and had been since
  v0.14.0** (FND-044). `bindings/fnec_py` declares its version in both
  `Cargo.toml` and `pyproject.toml`, and maturin stamps the *pyproject* one onto
  the built wheel — which is what `pip install` delivers and `pip show` reports.
  The v0.14.0 bump touched only `Cargo.toml`, so that release announced
  "fnec_py 0.4.0 → 0.5.0" and published a package calling itself 0.4.0, carrying
  0.5.0's breaking behaviour. The release checklist's own consistency command
  greps `Cargo.toml` files and so could not see it. Both files now say 0.6.0, and
  `scripts/check-binding-version.py` fails CI if they ever disagree.

- **The GUI's sweep stream is testable, and its message sequence is pinned**
  (FND-034). The body lived inline in `FnecGui::update`, where nothing could reach
  it: deleting any of its three `send` calls left the whole suite green, so the
  caveats added in the two previous changes were carried by review alone.

  It captures no `self` — only the deck text, the sweep bounds and the output sink
  — so `nec_gui::sweep_stream::run_sweep_stream` takes the sink as a `Sink` and
  tests drive it with a futures channel.

  The tests assert the **sequence**, not the presence of each message. The
  geometry caveats must arrive before the first point, because sending them at the
  end would still be "present" while failing to do their job; and the
  mid-sweep-failure path must send *two* caveat messages rather than at least one.
  That second distinction matters: `any()` was satisfied by the pre-loop send even
  with the failure-path aggregate deleted, so that sabotage survived until the
  assertion counted instead.

  One residual is recorded rather than glossed. The failure-path aggregate is
  pinned for presence and position, not content: the only deck that reaches that
  branch fails on its first point, so its aggregate is legitimately empty, and
  `solve_at` has no frequency-dependent failure mode to build a better fixture
  from. The completion path's aggregate *is* content-pinned — which closes the
  regression that matters, since sending the wrong producer with the right count
  and order previously passed the entire suite.

- **A GUI sweep now warns about the range it actually runs** (FND-042). The
  caveat panel evaluates the low-over-ground check at the deck's `FR` card
  frequency — right for a single solve, wrong for a sweep, whose range the user
  types into the UI. A deck with `FR` at 60 MHz and its antenna 0.634 m up is a
  comfortable 0.127 λ there; swept down to 14.2 MHz it sits at 0.030 λ, deep into
  the caveat's range, and nothing said so. `SweepJob::solve_at` emitted no
  geometry caveat at all.

`validate::swept_low_ground_caveat` now owns both the worst-case frequency choice
  and the annotation that says which points it applies to, so the CLI's distributed
  path and the GUI sweep describe the same range identically. They had already
  drifted: one named the affected count and the other did not, so a 14–60 MHz sweep
  read as wholly affected in one frontend and partly in the other. Both had also
  found the caveat by substring match, which breaks the moment its wording changes.

  Caveats are sent when the job is *prepared* rather than when it finishes — a user
  watching a long streaming sweep should not be told at the end that the antenna was
  too low for the whole range.

  The sweep panel carries **only** the frequency-dependent caveat. The deck-caveat
  strip above the tab already renders the topology and junction ones, which do not
  vary with frequency, so emitting the full set there printed the same sentence
  twice on one screen for a junction-fed deck — the normal case for a sweep that
  earns caveats at all.

  The test fixture is clean at its `FR` frequency and low across its swept range,
  so the two answers genuinely differ; without that the test would pass against
  the old behaviour. Its mirror asserts a sweep that stays high says nothing.

  Found in the review of #399 — the same miss class that PR fixed for `--hosts`,
  still live in the GUI.

- **A distributed run now carries the same pre-solve caveats as a local one**
  (FND-020). `--hosts` emitted the deferred-ground and unsupported-topology
  caveats but not `low_finite_ground_warning` or `feedpoint_at_junction_warnings`,
  so a dipole at 0.03 λ over `GN 2`, or one fed on a junction, came back through
  `--hosts` as bare numbers where every other path qualifies them.

  Done controller-side, not on the wire. These are pure functions of the deck, its
  geometry, the ground model and the frequency — all of which the controller holds
  before it dispatches anything — and a caveat computed worker-side goes silent
  against an older worker. Only what the worker's own solve actually *did* needs
  to travel, which is the FND-026 stamp warnings and nothing else.

  The low-ground check is the only frequency-dependent one, and a sweep has many
  frequencies. It trips below 0.1 λ, so the **lowest** frequency is the worst case;
  reporting there catches any sweep that dips below the threshold anywhere, and a
  count is added when only some points are affected. One line per swept point would
  have repeated a fixed geometric fact up to thousands of times.

  Both paths now call one producer, `validate::hallen_geometry_caveats`, so a
  caveat added there reaches both by construction. Demonstrated rather than
  asserted: a probe caveat added to the producer alone appears on the local *and*
  distributed routes of the real binary, with no call site touched.

  The caveats are gated on the Hallén solver, as the local path already gated
  them: the MPIE models junctions, loops and the surface wave correctly, and
  `--hosts --solver mpie` is reachable while FND-018 is open — so without the gate
  the topology caveat told such a user to re-run with the solver they were already
  using.

- **A distributed run no longer swallows the caveats only it can see** (FND-026).
  The worker built its `LD`/`TL`/`NT` stamp warnings and dropped them: a
  malformed card that every other frontend reports was silently ignored under
  `--hosts`. Unlike the result-shape checks the controller performs for itself,
  these have to travel on the wire — the controller never parses the deck's
  stamps, so a worker that discards them ends the story.

  `TaskResult::Ok` gains `#[serde(default)] warnings: Vec<String>`, the same
  treatment `exec_used` already had. Compatibility is gated in **both**
  directions, because a worker is a separately installed binary and a
  mixed-version pool is the normal case: a new controller reading an old
  worker's line (no field) and an old controller reading a new worker's (unknown
  field). The second test also stops anyone adding `deny_unknown_fields` later —
  it protects the next field as much as this one.

  A repeated caveat prints once per frequency point, matching the local CLI,
  which also re-prints stamp warnings per frequency: each distributed task
  re-parses the deck independently. Verified rather than assumed — a three-point
  local sweep of a malformed-`NT` deck prints the caveat three times.

  Gated across a real process boundary, not only through serde: the round-trip
  test serialises inside one build and so cannot catch a worker binary that never
  fills the field, while `a_skipped_card_warning_survives_the_wire_to_a_real_worker`
  dispatches to `fnec worker --stdio` as an actual subprocess.

  **Deployed workers still need upgrading.** An older one sends no warnings and
  the controller prints none — the field cannot conjure a caveat the worker never
  transmitted.

- **A receive-only deck was refused for a source it does not have** (FND-035).
  `validate::source_risk_geometry_error` read every `EX` card with no type
  filter, so a plane wave's NTHETA and NPHI — which live in the fields a driven
  source uses for tag and segment — could match a short fat segment and reject
  the deck outright. On **every** frontend, since the check reaches
  `geometry_error` and so `diagnose`.

  Demonstrated on a deck with no driven source anywhere: `exit 1`,
  `unsupported source-risk geometry: EX on tiny segment tag 1 seg 2`. There is no
  source on that segment. It now solves.

  `feedpoint_at_junction_warnings` had the same shape with a plane-wave-only
  skip, so an unrecognised `EX` type counted as a feedpoint and could earn a deck
  a junction-feed caveat it had not earned. Both now go through
  `nec_solver::feedpoints`, which is the FND-031 seam — a current source stays
  included in both, because a current source on a junction has its feed current
  split across the joined wires exactly as a voltage source does.

  `source_risk_geometry_error` now takes `&NecDeck` rather than `&[Card]` — a
  breaking change to `nec_solver`'s public signature. The crates are
  workspace-versioned and not published independently, so nothing outside this
  repository can be affected today; noted because that stops being true the day
  they are.

  An `EX` type fnec does not recognise is no longer refused *here*. It is still
  refused, by `build_excitation`, with a message that names the type instead of
  blaming a segment whose meaning for an unknown type is itself unknown — and
  that is what the GUI and the Python bindings already reported, since both call
  `build_excitation` before `geometry_error`. So this also makes the three
  frontends agree on a deck they previously described differently.

  Each negative control ships with a **positive control on the same geometry**:
  put a real source on that segment, or a real feed on that junction, and the
  check must still fire. Without them a check that had simply stopped working
  would pass. New fixture `corpus/receive-planewave-fat-segment.nec`, gated
  end-to-end at the CLI rather than only in a unit test, because this changes
  which decks are *refused*.

- **One answer to "which `EX` card is the feedpoint"** (FND-031). Eight sites
  decided this for themselves, with four different filters, and two of the
  differences were bugs.

  The worker skipped every `EX` that was not type 0 — while `build_hallen_rhs`,
  which the worker calls, *drives* a type-5 card as a delta gap. So it assembled
  the physics for a type-5 source, solved it, and then refused to read the answer,
  reporting "no EX type-0 card found in deck". A deck the CLI and the Python
  bindings both solve to **74.242874 + j13.899516 Ω** was rejected outright by
  `--hosts`. And the GUI and the bindings filtered nothing at all, so a plane wave
  standing ahead of the driven source had its NTHETA/NPHI read as a feedpoint tag
  and segment — grid dimensions reported as an antenna location.

  `ExcitationKind::feedpoint_role()` now classifies the card and
  `nec_solver::feedpoints` / `first_delta_gap_feedpoint` answer the deck-level
  question. It is a **classification, not a predicate**, because no single boolean
  serves every caller: a type-4 current source is not a delta gap but *is* a
  feedpoint, priced from the solved port voltage and corpus-pinned at
  74.23 + j13.9 Ω under PH8-CHK-001 — a seam filtering on "voltage source" would
  have silently deleted that row. (That was the design first proposed here, and
  review caught it before any code was written.)

  The three RHS builders keep their own loops and their `UnsupportedType` error
  channel — building an RHS is a different question from naming a feedpoint — but
  no longer their own *policy*: all three `match` on the role with **no wildcard
  arm**, so a new excitation type breaks the build in three places instead of
  falling into whichever branch happened to be last.

  The distributed diagnostic added in #395 and the worker's extraction are now the
  same call. That parity was a comment asking two files to be kept in step, and
  they diverged twice inside a single review.

  New fixture `corpus/dipole-planewave-then-source-51seg.nec`, which pins
  *resolution* rather than a value: a deck carrying a plane wave routes to the
  receive path, so its driven-feedpoint impedance is degenerate by design.

  One behaviour change comes with it, in the right direction but not yet with the
  right words: a current-source-only (`EX 4`) deck now **errors** in the GUI and
  `fnec_py`, where the old unfiltered loop solved a zero RHS and reported `V/I`
  from it as an impedance. Neither frontend can price a current source — that
  needs the solved port voltage — but the message they raise names the wrong
  problem, so FND-038 tracks giving them the named rejection the worker got here.

  Three sites are **not** covered and are recorded rather than quietly left:
  `validate::source_risk_geometry_error` (FND-035 — no filter, so a plane wave can
  spuriously *reject* a valid receive deck on every frontend),
  `feedpoint_at_junction_warnings` (FND-035), and the MPIE session's own lookup
  (FND-037, which admits current sources and unrecognised types as delta gaps).
  They change which decks are refused, which wants its own negative controls.

- **Every frontend now warns when a result is physically impossible** (FND-014).
  A passive antenna cannot have a negative input resistance, so `Re(Z) < 0` on the
  Hallén path means the reported impedance is unreliable. The check was private to
  the CLI with a single call site, so the GUI, the Python bindings and a `--hosts`
  run returned that number with no caveat at all. An inverted-V fed away from its
  apex reports **-5.973 - j1122.555 Ω**: the CLI has flagged it since PH9-CHK-005,
  the other three said nothing.

  `nec_solver::validate::negative_resistance_warning` is now the shared seam,
  reached by every frontend — the worker's share of it runs controller-side, for
  the reason below. It is the one *post*-solve check in that module and is documented as the exception —
  deliberately **not** part of `diagnose`, which the GUI calls on every keystroke
  with no matrix in hand and which must stay solve-free.

  Three details worth stating, because each was a decision rather than an
  oversight. The distributed path is covered **controller-side**, not in the
  worker: a worker is a separately installed binary, so a warning that lived only
  there would go silent against an older one — reproducing the finding under
  version skew. The MPIE arm stayed in the CLI, because its message is a claim
  about that binary's solver arsenal rather than about the deck. And the GUI sweep
  gets **one aggregate line** naming how many points went negative, not a
  per-point warnings field: the cause is a property of the geometry and does not
  vary with frequency, so per-point strings would repeat one diagnosis up to
  `MAX_SWEEP_POINTS` times while restating values the point already carries.

  New fixture `corpus/inverted-v-negative-r-freesp.nec` — deliberately *not* a
  parity case; its number is known-wrong, and that is the point.

  A known trade, recorded rather than discovered later: `fnec_py`'s sweep now
  raises one `UserWarning` per negative point, because the message embeds `Re Z`
  and its dedup keys on exact text (FND-032). The GUI aggregates for this reason;
  the bindings do not yet.

  `NaN` is deliberately **not** caught here: the sentence would read "has negative
  resistance (Re Z = NaN Ω)" and blame a junctioned geometry that is not the cause.
  A `NaN` impedance is a non-converged solve and wants its own diagnostic
  (FND-030). One predicate, `is_negative_resistance`, now decides this for the
  shared seam, the CLI's MPIE arm and the sweep counter alike, so they cannot
  disagree about a single value.

- **A negative-resistance deck could be sent to a solver that rejects it**
  (FND-029). The diagnosis offered `--solver mpie` as a cross-check without asking
  whether the MPIE can take the deck. `validate::mpie_compatible_deck` exists for
  exactly this and `unsupported_topology_warning` already obeyed it, but the
  CLI-private copy did not — so a junctionless deck carrying an `LD` card was
  pointed at a solver that refuses `LD` outright. Latent while the check reached
  one frontend; promoting it to a shared seam would have spread it to three more.
  Found in design review, before the code was written.

- **All frontends now apply the same deck stamps** (FND-015, FND-023).
  `build_nt_stamps` was called only from the CLI, so an `NT` deck solved to
  70.633 + j14.009 Ω there and 74.243 + j13.900 Ω — the plain-dipole answer —
  through the GUI, the Python bindings and the remote worker. `LD`, `TL` and `NT`
  now come from one seam, `nec_solver::build_deck_stamps`, used by all six
  assembly sites and by the GUI's warnings-only path, so the caveats shown and the
  stamps applied can no longer describe different card sets.

  The same change fixes `--exec gpu` discarding stamps (FND-023). The GPU-resident
  path re-fills and solves on the device, so every host-side stamp is dropped when
  it succeeds — and both gates guarding it asked *which card types are present*
  from separately maintained lists, the CLI's omitting `NT`. They now ask
  `DeckStamps::is_identity()` — whether the deck actually stamps anything — and the
  CLI additionally declines the device path when `--loads-config` supplied Laplace
  loads, which the gate cannot see. Verified on hardware: an `NT` deck gave
  74.234 + j13.898 Ω under `--exec gpu` against 70.633 + j14.009 Ω on the CPU, and
  `--exec gpu --loads-config …` returned the *unloaded* 74.234 + j13.898 Ω where
  CPU gives 442.655 − j971.944 Ω — a load the user explicitly passed, discarded.
  Both now match `--exec cpu` exactly, while a deck that stamps nothing still
  reaches the device: a plain dipole gives 74.234166 Ω under `--exec gpu` against
  74.242874 Ω on the CPU, the f32 signature that shows the gate is declining
  rather than the adapter being absent — and that 74.234166 is exactly what the
  `NT` deck used to return.

  Two smaller behaviour changes come with it. A deck whose `LD` or `TL` cards
  stamp *nothing* — a malformed `TL`, an unsupported `LD` type, an `LD 4` with
  R = X = 0 — is no longer forced onto the CPU by the mere presence of the card,
  so under `--exec gpu` it now takes the device path and its f32 arithmetic
  (within the divergence rejection added in #373). And an unsupported-`LD`
  warning was pushed once per matching segment, so an `LD 9` spanning 21 segments
  printed 21 identical lines; the seam deduplicates, and it prints once.

  **Deployed remote workers are not updated by this.** A worker is a separately
  installed binary; an older one keeps ignoring `NT`, so a mixed pool will disagree
  with a local solve until every worker is upgraded.

- **`--hosts` with `--loads-config` silently dropped the loads** (FND-025).
  `run_distributed_solve` takes no Laplace parameter and the worker protocol
  carries no field for them, so a distributed run of a loaded deck returned the
  *unloaded* impedance — FND-023's signature one layer up, and on the CPU path
  too. The combination is now rejected before any host is contacted. Found by the
  strong-model review of the FND-023 fix, which declined to accept "the
  user-passed-load-discard class is fixed" while this one stood.

- **`--hosts` with `--ground-solver sommerfeld` silently ignored the flag**
  (FND-027). Same shape, second instance: `run_distributed_solve` does not take
  it and the worker derives its ground model from the deck alone, so a
  distributed run returned the uncorrected reflection-coefficient impedance with
  no warning. On `corpus/dipole-gn2-near-ground-51seg.nec` that is
  92.266 + j13.617 Ω against 95.524 + j12.166 Ω with the PH9-CHK-006 correction —
  a 3.26 Ω change for a flag the user passed explicitly. Also rejected now. The
  review found this by asking which *other* flags the distributed path drops the
  same way, rather than accepting the first instance as the whole defect; the
  remaining flags were swept and are either rejected loudly by the worker,
  applied before the branch, or performance-only.

- **The path-inventory checker was satisfied by its own comment.** It validated
  only backticked names of 15+ characters, so the invented symbol `solve_task` —
  a function that never existed — sat in the inventory and the findings ledger
  through two reviews without the gate noticing. Tightening it to any snake_case
  identifier exposed two further ways it confirmed whatever it was given: it
  substring-matched, so a truncated name resolved to the real one it prefixes, and
  it searched `docs/` and `scripts/`, so a name resolved either to its own mention
  in the file under check or to the examples in the checker's own explanatory text.
  Now whole-word, source-tree-only, and sabotage-verified against both (FND-022).

- **A distributed solve no longer skips pre-solve validation** (FND-013). `--hosts`
  returned from `main()` *before* the validation block, and the worker went from
  `build_geometry` straight to the solve, so a deck the CLI refuses locally was
  dispatched to every worker and solved — the worker returned 49.53 − j173.05 Ω for
  wires crossing mid-span. The check is now one shared block **above** the
  `--hosts` branch, which also puts it ahead of `WorkerPool` construction: the pool
  spawns an SSH process per host the moment it is built, so a check placed inside
  the distributed function would contact every host before noticing the deck was
  never solvable. The worker validates independently too — it is a separately
  installed binary, possibly a different version, so a controller cannot speak for
  it — reporting `UnsupportedConfig` rather than a geometry error, which would have
  crossed the wire mislabelled as `parse_error`. The distributed path also now
  emits the unsupported-topology caveat it never saw.

- **The CLI test suite no longer leaks temp decks.** Six integration-test files
  wrote uniquely-named decks to the system temp directory and never removed them,
  while the other eighteen cleaned up. Repeated `cargo test --workspace` runs left
  **437** stray `fnec-*` files in `/tmp`. They now use a shared `common::TempDeck`
  guard that deletes on drop — so it also cleans up after a *panicking* test, which
  the trailing `fs::remove_file(&path)` convention does not. A full workspace run
  now leaves 6 fixed-name GUI fixtures that overwrite rather than accumulate,
  down from ~437 unique files (FND-017).

- **The GUI's warning marker rendered as an empty box.** `⚠` (U+26A0) has no glyph
  in iced's default font, so every caveat in the Solve panel had been drawn with a
  tofu box in front of it — since well before the caveats strip existed. Found by
  actually looking at a screenshot; the tests could not see it. All three sites now
  use plain `warning:` / `error:` text, matching what the CLI prints. Box-drawing
  (`─`) and `Ω` do render, so the rule is to stay inside what the shipped font
  covers rather than assuming symbol support.

- **Corpus reference values now carry per-case provenance, and the file-wide claim
  they replace was wrong for 37 of 48 cases.** `reference_engine_version` asserted
  one engine version for the whole corpus; the cases were in fact last produced by
  builds ranging from 0.2.0 to 0.9.0, across ten dates. Each case now records
  `last_produced_on` and `last_produced_in` — the date and workspace version of the
  commit where its stored values *last changed* — derived from git history by
  `scripts/derive-corpus-provenance.py`, which replays every commit that touched
  the file and fingerprints each case's own subtree. A new CI step re-checks it, so
  a case added without provenance, or one whose values change without a re-derive,
  fails the build. Derived rather than asserted: it can be re-run and disagreed
  with.

- **The GUI shows deck caveats on every tab, not just the Solve panel.** The
  warnings `nec_solver::validate` produces describe the *deck* — its geometry, its
  ground model, its topology — but only `impedance_view` rendered them, so a user
  who ran nothing but sweeps or patterns saw none of it and had no way to know the
  numbers on screen were flagged as unreliable. A deck-caveats strip now sits above
  the tab content, populated by `solve::deck_warnings` whenever any action reads
  the deck, and cleared when the deck path changes so stale caveats never describe
  the wrong file. The strip and the Solve panel read from the same source, so they
  cannot disagree.

- **The negative-resistance warning no longer blames a cause the deck cannot
  have.** It offered "commonly a junctioned-geometry limitation (see PH9-CHK-002)"
  unconditionally, so a deck containing a *single straight wire* — no junction
  anywhere in it — sent the reader after a cause that is not present. A badly
  under-segmented 40 m wire (3 segments over ~1.9 λ) reproduces it at −162.5 Ω.
  The explanation is now chosen from `nec_solver::validate::has_wire_junction`,
  which uses the same merged-conductor grouping the junction warning does, so the
  two cannot disagree; a junctionless deck is told the usual cause does not apply
  and pointed at a cross-check instead. Where a junction really is present the
  junction explanation is unchanged.

### Docs

- **Roadmap `GAP-015` corrected from Done to Partial** (FND-006). Its acceptance
  criterion asks for Markdown project import/export *"with documented schema,
  round-trip stability tests, and explicit CLI/API entry points"*, and it was marked
  Done citing only the library functions and their tests. No frontend imports
  `nec_project` — `apps/nec-cli/Cargo.toml` declares it as a dependency that is
  never used, which is now logged separately as FND-016. The library half is
  delivered; the half the criterion names is not, and the row now says so.

## [0.14.0] — 2026-08-23 — Frontend validation parity + GPU and MPIE correctness

Every finding of the 2026-07-19 project review is closed in this release. The
headline is correctness rather than features: three separate paths were returning
answers that were quietly wrong — `--solver mpie` on one legal way of writing a
deck, `--exec gpu` on larger decks, and the GUI and Python bindings on geometry the
CLI refuses outright.

**Breaking:** `nec_model::card::NeCard` is renamed, and the Python bindings now
raise on decks they used to solve. Both are covered by the migration guide in
`docs/releasenotes.md`.

### Added

- **`nec_solver::validate` — a shared pre-solve validation module** (review-260719
  FIND-004/006/007/008, step 1 of 3). The hard geometry rejections (wires crossing
  mid-span, a source on a degenerate segment, a wire reaching an active ground) and
  the geometry/ground warnings lived inside the CLI binary, where the GUI and the
  Python bindings could not reach them — so a deck the CLI refused outright solved
  silently and wrongly on the other two frontends. They are now pure functions of
  `(&NecDeck, &[Segment], &GroundModel, freq_hz)` that *return* diagnostics
  (`nec_model::ValidationDiagnostic`) instead of printing them, with `diagnose()`
  as the one-call entry point for a frontend. The CLI delegates to them and its
  message text is unchanged, byte for byte, as its contract tests require. Wiring
  the GUI and the Python bindings follows in separate changes.

### Changed

- **`nec_model::card::NeCard` is renamed `NearFieldCard`** (review-260719
  FIND-012). `Card` has both `Ne(NeCard)` and `Nh(NeCard)`: NEC-2 gives the two
  cards an identical field layout, so one struct is right — but naming and
  documenting it as the *electric* field card meant an `NH` card was carried in a
  type whose docs said it meant something else. The struct now describes the
  observation grid, which is what it holds, and the requested quantity stays where
  it belongs: the `Card::Ne` / `Card::Nh` variant. A breaking change to
  `nec_model`'s public API; every in-repo consumer (parser, GUI deck writer, the
  Python bindings) is updated and compiles.

- **A distributed sweep now uses every worker at once** (review-260719 FIND-009).
  `WorkerPool::dispatch` blocks until one worker answers, and `--hosts` drove it
  one frequency point at a time, so N−1 workers sat idle: M points cost
  `M × latency` instead of `M/N × latency`. The new `dispatch_batch` gives each
  worker a thread pulling from a shared cursor, so a fast node takes more work than
  a slow one without any scheduling policy. Measured on 8 tasks over 4 local
  workers: **9.25 s → 2.47 s** (3.75×, against an ideal of 4×).

  Failure handling is unchanged: a worker that errors is dropped from the pool and
  the task it was holding is retried on a survivor rather than lost. Results are
  indexed by task, so report order does not depend on which worker finished first —
  only the `worker=` diagnostic label varies, which was never a stable contract.

- **The wgpu device is built once per process, not once per kernel call**
  (review-260719 FIND-005). Every GPU entry point independently called
  `Instance::new` + `request_adapter` + `request_device`, so a frequency sweep paid
  full device initialisation **twice per frequency point** — 20 initialisations for
  a 10-point sweep, measured. A 10-point sweep of a 301-segment dipole under
  `--exec gpu` goes from **5.30 s to 4.17 s** (21 %), with `--exec cpu` unchanged at
  1.70 s over the same runs. The solved impedances are byte-identical to the
  pre-change build.

  Two call sites deliberately keep building their own: the two
  `force_fallback_adapter: true` probes, which select the software adapter on
  purpose, and `microbench_zmatrix_dispatch`, which **times** device acquisition as
  one of its reported metrics — handing it a cached device would corrupt the
  measurement it exists to produce. A device lost mid-run drops the cached context
  so the next solve rebuilds, rather than pinning the rest of the process to the
  CPU fallback.

  Note this does not make `--exec gpu` faster than `--exec cpu` on this hardware
  (4.17 s vs 1.70 s); it removes one specific overhead. The accuracy gate above
  costs part of it back.

- **The defensive guards flagged as untested now have tests** (review-260719
  FIND-014/016), 26 cases across four modules. `nec_solver::network` had **no tests
  at all** — its seven `NT` rejection paths (short card, non-integer identifiers,
  non-numeric admittances, either endpoint missing, both endpoints on one segment,
  singular admittance matrix) are now covered, along with the supported path
  checked against `[Z] = [Y]⁻¹`. `nec_solver::tl` gains its five rejection paths,
  and `nec_project::from_markdown` its six. Each rejection mutates one field of a
  fixture that is separately asserted to be *accepted*, so a failure means the
  guard fired rather than the fixture being broken.

  For FIND-016, `probe_capability` still needs a reachable host and stays untested,
  but the part that can actually be wrong — parsing what comes back — is split out
  of the SSH call into `parse_cpu_threads` / `parse_gpu_available` and tested,
  failure defaults included. Those defaults matter: a node parsed as having zero
  threads would drop out of scheduling entirely, and an erroring GPU probe must
  never promote a node to GPU-capable.

- **A test-infrastructure race that could fail any CI run.** `exec_modes`' drop-in
  alias paths were keyed on `(alias_name, nanoseconds)`, but six matrix tests loop
  over the *same* alias-name list in parallel — two threads could take one name in
  the same nanosecond, and the loser's `fs::copy` then failed with `ETXTBSY`
  because the winner was already executing that file. It took down this release
  branch's first coverage run. The key now carries a process-wide counter. (#379)

### Fixed

- **`--solver mpie`: feedpoint impedance no longer depends on `GW` direction.**
  The MPIE's nodal basis takes its reference current direction from the incidence
  order of the fed node's two arms. When the driven segment's `GW` card is written
  *outward* from the shared node — an apex-fed inverted-V entered as two `GW`
  cards that both start at the apex — that direction opposes the segment's own
  tangent, and the CLI's rebuilt `V/I` came out negated: a physically impossible
  **negative resistance** (−40.6 − j8.0 Ω) for the same antenna the end-to-start
  form solved correctly at +40.7 + j8.1 Ω (nec2c 43.5 + j12.4). The solve is now
  re-referenced to the `EX` source polarity. The library's own `MpieSolution::z_in`
  was always correct — only the CLI rebuild lost the sign — so no validated
  library result changes. The negative-resistance tripwire is now also armed on
  the MPIE path (previously `hallen`-only), so a defect of this shape cannot pass
  silently again.

- **`--exec gpu` no longer reports a diverged solve as a result.** The GPU-resident
  Hallén solve is f32, and its normal-equations form squares the condition number,
  so it loses accuracy as the segment count grows. Nothing checked the answer: on a
  301-segment λ/2 dipole one frequency point came back at 101 Ω against the CPU's
  75 Ω, another at **−1.98 Ω** — a negative resistance for a passive antenna — and
  a 151-segment deck was off by 7 %. The 2 Ω tolerance the shader header claims
  "the host validates" existed only in a 51-segment parity test, never in the
  production path.

  The shader now returns its own relative residual `‖y − Mx‖ / ‖y‖` and the host
  rejects a non-converged solve, falling back to the f64 CPU solve with a warning.
  The threshold (`1e-4`) is derived from measurement rather than picked: across
  51–301 segments every accurate solve sits at or below 7e-5 while the smallest
  inaccurate one is 8.9e-4, an order of magnitude clear. After the gate, `--exec
  gpu` matches `--exec cpu` within 0.6 Ω at every size tested, and the negative
  resistances are gone.

  The residual pass and the added CPU fallbacks cost some of the device-caching
  win back: the 10-point 301-segment sweep runs ~4.9 s against 4.2 s ungated and
  5.3 s before either change. `--exec cpu` remains faster (~1.7 s) on this
  hardware.

- **`--ground-solver sommerfeld` diagnostics now tell the truth about what ran.**
  The low-height finite-ground warning ("…does not model the Sommerfeld surface
  wave") was unconditional on height and ignored `--ground-solver`, so it fired
  even when the Sommerfeld correction *had* been applied — denying the very
  surface wave the reported `Z` included. It is now suppressed once the correction
  actually applies. Conversely, the correction covers straight wires only and used
  to decline bent or mixed geometry **in silence**, leaving the user believing they
  had the surface wave when they had the reflection-coefficient result; a declined
  request now warns and points at `--solver mpie`. A request that was never made
  is not a decline, and a declined request keeps the low-height warning.

- **The GUI now applies the same pre-solve validation as the CLI** (review-260719
  FIND-006/007, step 2 of 3). It went straight from `build_geometry` to
  `solve_hallen`, so a deck the CLI refuses outright — wires crossing mid-span, a
  source on a degenerate segment, a wire reaching into an active ground — solved
  silently in the GUI and displayed a wrong impedance. All three GUI solve paths
  (impedance, sweep, currents/pattern) now reject it, so the pattern views cannot
  draw a plausible-looking result for geometry the impedance view refuses. The
  warning set is widened to the CLI's: low-antenna-over-finite-ground, feedpoint on
  a junction, unrecognised `GE I1`, and the parser's own warnings, which the GUI
  used to discard. Both frontends read their message text from
  `nec_solver::validate`, so they can no longer drift apart.

  Still GUI-side gaps, unchanged here: the sweep, pattern and currents views render
  no warnings (only the impedance panel does), and the GUI remains Hallén-only —
  the topology warning names the CLI's `--solver mpie` rather than offering a
  solver choice.

- **The Python bindings now validate too, and are finally covered by CI**
  (review-260719 FIND-004, step 3 of 3). `fnec_py.solve_deck_str` /
  `sweep_deck_str` went from `build_geometry` straight to `solve_hallen`, so a deck
  the CLI refuses outright returned a plausible-looking impedance; those decks now
  raise `RuntimeError` with the same message the CLI prints. Non-fatal caveats — an
  unreliable topology, a very low antenna over finite ground, parser warnings, and
  the load/TL builder warnings the bindings used to discard — are raised as Python
  `UserWarning`s, so they show by default and can be filtered or escalated with the
  standard `warnings` module. A sweep emits each distinct caveat once rather than
  once per frequency point.

  `bindings/fnec_py` is excluded from the cargo workspace, so **every `--workspace`
  CI job skipped it** — it could be broken by any `nec_solver` API change with
  nothing noticing until someone built a wheel by hand. A new `python bindings` job
  runs fmt, clippy `-D warnings`, a maturin build and the pytest suite. Python is
  pinned to 3.13, the newest CPython pyo3 0.23 supports.

- **`docs/cli-guide.md` parity sweep** (review-260719 FIND-001/002/003). The guide
  had drifted roughly ten minor versions behind the code. The synopsis and options
  table now match the binary's own `USAGE` — `--ground-solver`, `--output-format`
  and `--hosts` were entirely undocumented — `--ex3-i4-mode` is documented as the
  obsolete no-op it is rather than as a behavioural switch, and the card
  quick-reference is resynced with the authoritative `docs/card-support-matrix.md`
  (EX 1–5, PT, NT, TL, GN, NE/NH). Also corrected: the `hallen` section still said
  non-collinear geometry was rejected, the `mpie` section still scoped near-ground
  currents to a straight horizontal wire, the TL section still called lossy lines
  deferred, and the Notes still called GPU acceleration unwired. Two claims were
  written from *measured* behaviour rather than existing prose — `--hosts` has no
  local fallback (missing file or unreachable worker exits 1), and
  `--ground-solver sommerfeld` corrects any straight wire, not only a horizontal
  one, while declining bent geometry silently.
- The 2026-07-19 multi-agent project review is now committed
  (`docs/dev/reviews/review-260719.md`) with a remediation-status block, so it
  reads as a dated snapshot rather than an open worklist.

- **Corpus reference provenance corrected** (review-260719 FIND-015, though not in
  the way it was reported). `reference_engine_version` records *which engine
  produced the stored reference values*; the workspace being at 0.13.0 does not
  make 0.13.0 the right value, and bumping it would have claimed a regeneration
  that never happened. The real problems were that 48 cases accumulated across many
  releases all carry one provenance string, and that the documented example in
  `docs/nec-requirements.md` showed two keys (`schema_version`, `last_updated`) the
  file does not have. The field is now marked as the initial baseline, a
  `provenance_note` records that later cases are not separately stamped, and the
  example matches the real file. Per-case provenance remains unrecorded — the
  substantive gap, and larger than the review item.
- **The 2026-07-19 review is fully dispositioned.** Every finding is now closed
  with evidence, including the four that did not become code changes, each with the
  reasoning recorded rather than silently dropped. The review itself is committed at
  `docs/dev/reviews/review-260719.md`.

## [0.13.0] — 2026-08-23 — Laplace loads + Leeson taper + project-quality hardening

Two new user-facing features from the pymininec cross-validation review, plus a
substantial project-quality pass (CI, traceability, coverage). The default Hallén
solver and the validated corpus are unchanged.

### Features

- **Laplace-domain loads** (`--loads-config <file.toml>`): an arbitrary rational
  series load `Z(s) = N(s)/D(s)` (`s = jω`), generalising the LD 0–5 lumped loads
  and covering matching networks, traps with parasitic R, and curve-fitted loads.
  Hallén/pulse paths; reproduces the equivalent `LD` network to numerical
  tolerance. Rejected on `--solver mpie`. (#357)
- **Leeson step-tapered-radius correction** (`fnec taper --sections "<dia>,<len> …"`):
  replaces a stepped-diameter (telescoping-tubing) element with its equivalent
  uniform-diameter element, per D. B. Leeson, *Physical Design of Yagi Antennas*
  ch. 8. Validated to the digit against the book's worked example. (#360)

### Fixes

- **GPU readback** now degrades to the CPU path on a device-lost / dropped-map
  failure after dispatch instead of panicking. (#355)

### Project quality

- **Core CI** — fmt / clippy `-D warnings` / test / `cargo audit` / `cargo deny` /
  docs contract on every PR (previously local git hooks only), a **coverage floor**,
  SHA-pinned actions, and least-privilege workflow permissions. (#350, #352, #356)
- **Machine-enforced requirements traceability** — a machine-readable
  `docs/project/requirements.toml` register bound to tests by `// VERIFIES: <ID>`
  comments, with a GAP/dangling checker in the test gate and a generated matrix. (#354)
- **Docs** — pymininec reference, frontmatter-validator scope fix + stale
  cli-guide version, the 2026-08-21 gap review, and the Sommerfeld–Norton "Level 2"
  DCIM scoping + a validated Python prototype (studies). (#351, #353, #358–#363)

### Known limitations

- Unchanged from 0.12.0. `fnec taper` is for linear, essentially unloaded elements
  within ~±15 % of self-resonance; Laplace loads and the taper subcommand are CLI
  features (not wired into the GUI).

## [0.12.0] — 2026-07-13 — GPU 3-D antenna workbench (GUI redesign) + pre-release correctness fixes

The headline is a full redesign of `nec-gui` into a **GPU-accelerated 3-D antenna
workbench** on iced 0.13 (10 phased increments, GUI-CHK-001..010), plus a
pre-release review pass that fixed several latent solver-math and GUI-honesty
bugs. The CLI, the default Hallén solver, and the validated corpus are unchanged.

### Added

- **3-D antenna workbench GUI (`nec-gui`).** A resizable single-window workbench:
  a persistent **wgpu 3-D viewport** (wire geometry, current-magnitude coloring,
  translucent radiation-pattern lobe; orbit / zoom / pan / reset; axes & ground-grid
  toggles) beside task tabs. (#315–#321, #336)
- **Streaming frequency sweep with a live chart.** The sweep streams point-by-point
  into an **SWR / |Z|-vs-frequency canvas plot** with a draggable frequency cursor
  and readout. (#332, #333, #334)
- **Visual deck editor.** Edit `GW` wires and `EX`/`GN`/`LD`/`FR` control cards in
  tables (pick-lists for enumerated fields), **add/remove** cards, **undo/redo**
  (Ctrl+Z / Ctrl+Shift+Z), live 3-D preview on every valid edit, **Apply + Solve**,
  and **Save / Save as…**. Backed by a new NEC deck **writer** (`deck_write`,
  round-trip-tested against the corpus). (#323, #324, #326, #328, #329, #331)
- **Native file dialogs** (Browse / Save as, via `rfd`) and **session persistence**
  (deck/vars paths, sweep range, chart metric, camera pose, view options restored on
  restart). (#337, #338)
- **RP `XNDA` A-digit average power gain** (#314) and **spherical `NE`/`NH`
  near-field grids** (#313).
- **`--solver mpie` composition** with JSON output, frequency sweep, and
  ground-pattern paths; the Hallén guard now recommends `--solver mpie` for
  degree-3 junctions and closed loops. (#311, #312)

### Fixed

- **LD type-5 wire conductivity** used a dimensionally wrong formula (Ω·m, ~10⁴–10⁵×
  too small, no reactance). Replaced with the exact round-wire skin-effect internal
  impedance (DC ↔ surface-impedance limits). (#340)
- **MPIE ignored the `EX` source voltage** — feedpoint impedance was scaled by V for
  any deck with a source voltage ≠ 1 V. Now voltage-independent. (#341)
- **MPIE** warns on mixed wire radii (single-radius kernel) and no longer
  double-counts the Sommerfeld surface wave under `--ground-solver sommerfeld`. (#342)
- **`AXIAL_RATIO`** reported `|Eθ|/|Eφ|` (not an axial ratio); now the Stokes-parameter
  polarization axial ratio. (#343)
- **GUI no longer solves silently-wrong geometries.** Junction/loop/deferred-ground
  and unsupported-load caveats are surfaced on the Solve tab (the GUI runs Hallén;
  it recommends the CLI `--solver mpie`). (#344)
- **GUI robustness:** a runaway sweep is capped instead of freezing the app (#345);
  the MPIE rejects empty/zero-length geometry instead of panicking (#346); the
  viewport pane no longer overflows the window (#322); magnitude bars render as
  widgets, not tofu block characters (#319).

### Known limitations

- The GUI runs the **Hallén** solver only; junctions, closed loops, and near-ground
  currents over finite ground are flagged with a ⚠ and should be solved with the CLI
  (`fnec --solver mpie`).
- GUI rendering (the wgpu viewport + canvas plot) is exercised by the visual gate
  (`cargo run -p nec-gui`); all non-rendering logic is headless-tested
  (workspace coverage 82%).

## [0.11.0] — 2026-07-10 — MPIE second solver + Sommerfeld surface-wave ground

This release ships two large Phase-9 efforts. The headline is a **second solver**,
`--solver mpie` (a subsectional mixed-potential EFIE), which retires the three
frontiers the Hallén architecture structurally could not reach — degree-3 (T/Y)
junctions, closed loops, and near-ground currents/patterns. Alongside it, the
**Sommerfeld surface-wave ground** is available on the Hallén path as an opt-in
feedpoint-Z correction, `--ground-solver sommerfeld`. Every increment was
de-risked in Python and validated against live nec2c before landing. The default
Hallén path is unchanged, so the validated corpus is untouched.

### Added

- **`--solver mpie` — subsectional mixed-potential EFIE (PH9-CHK-007).** An opt-in
  second solver with a piecewise-linear (triangle) current basis that carries the
  vector and scalar potentials separately (the Hallén reduction folds the scalar
  potential into a per-wire homogeneous term and so cannot represent it). This
  reaches geometry classes the Hallén path guards or mis-solves:
  - **Degree-3 (T/Y) junctions** — Kirchhoff's current law is satisfied by the
    junction basis itself (N−1 arm-pair "dipole" bases per degree-N node), so a
    symmetric Y-junction converges monotonically to nec2c (R 68.75/69.33/69.84 at
    N=10/20/40 → 71.5 Ω) where the entire-domain Hallén prototype *diverged* past
    80 Ω.
  - **Closed loops** — a cyclic all-degree-2 chain the same basis handles with no
    endpoint condition; a 1λ square loop converges to nec2c 109.7 − j146.2.
  - **Near-ground currents/patterns (Sommerfeld in the Z-matrix)** — the reflected
    mixed-potential kernels (horizontal) and a reflected-E-field-dyadic reaction
    (any straight or bent orientation) put the surface wave into the *current
    solution*, not just the feedpoint Z: a horizontal λ/2 dipole over GN2
    reproduces nec2c to <8 %, a vertical dipole to ~7 %, and an inverted-V captures
    the surface-wave shift.
  - Because it keeps the scalar potential, the MPIE's absolute reactance tracks
    nec2c without the Hallén ~32 Ω offset (a λ/2 dipole reports 74 + j42 Ω vs
    Hallén's 74 + j5 Ω). Free-space patterns/gain reuse the existing radiation sum
    (λ/2 dipole 2.15 dBi, planar Y-junction 1.94 dBi = nec2c). Models geometry +
    voltage sources (`EX` type 0); `LD`/`TL`/`NT`, plane waves, and current sources
    are rejected on this path. New `crates/nec_solver/src/mpie.rs`; wired through
    `--solver mpie`. See `docs/mpie-solver-scope.md` and `docs/cli-guide.md`.

- **`--ground-solver sommerfeld` — Sommerfeld surface-wave near-ground impedance
  (PH9-CHK-006).** An opt-in correction on the Hallén path that upgrades the
  feedpoint impedance of a straight wire over finite ground from the
  reflection-coefficient (RCM) approximation to the exact Sommerfeld half-space
  (nec2c GN2), including the surface-wave sign flip below ~0.05 λ that the scalar
  model gets wrong. Default `rcm` is unchanged (zero behavior change). Built on a
  validated 1-D Sommerfeld reflected-field kernel and its arbitrary-orientation
  dyadic (`crates/nec_solver/src/sommerfeld.rs`). See
  `docs/ph9-chk-006-sommerfeld-ground.md`.

### Notes

- The MPIE and Sommerfeld ground solver are **opt-in**; the default Hallén +
  scalar-Γ paths are unchanged, so the validated corpus and every existing gate are
  untouched (zero regression).
- Out of scope on the MPIE ground path: wires that cross the `z = 0` plane (buried
  geometry, a different physical problem).

## [0.10.0] — 2026-07-08 — Phase 9: general junction basis, junction receive/current-source, near-ground impedance
### Added

- **Near-ground impedance accuracy boundary + low-height guard (PH9-CHK-006)** — a
  height sweep against nec2c (reflection-coefficient GN0 vs exact Sommerfeld GN2)
  characterizes where fnec's finite-ground impedance is trustworthy: it is genuinely
  accurate (≈ Sommerfeld, within ~10 %) for antenna heights ≥ ~0.2 λ — now gated
  (`ground_impedance.rs`: a 0.25 λ horizontal dipole ΔR +9.9 Ω vs Sommerfeld +11.0)
  — and degrades below, becoming unreliable under ~0.1 λ where the surface wave
  dominates (at 0.025 λ the reflection-coefficient ΔR is −24 Ω vs the +9 Ω truth — a
  sign error). `warn_if_low_finite_ground` now warns when the lowest conductor point
  is below 0.1 λ over finite ground that the impedance is a reflection-coefficient
  approximation with no surface wave. This completes PH9-CHK-006's acceptance
  criteria (accurate class gated, low-height/buried classes guarded, boundary
  documented). Notably, angle-dependent Fresnel (nec2c GN0 RCM) is **not** a
  worthwhile increment — fnec's scalar Γ already reproduces it where it matters; only
  the Sommerfeld/Norton surface wave (nec2c GN2) closes the < 0.1 λ gap, and it stays
  deferred. See `docs/ph9-chk-006-sommerfeld-ground.md`.

### Fixed

- **Near-ground feedpoint impedance had the wrong-signed ground effect
  (PH9-CHK-006)** — the method-of-images reflection term in the Hallén Z matrix
  (`matrix.rs::image_segment`) used the image current `(Jx, Jy, −Jz)` instead of the
  correct PEC image `(−Jx, −Jy, +Jz)` (Balanis Table 4-1) — the exact negation. So
  the reflected contribution entered *every* near-ground impedance with the wrong
  sign: a horizontal λ/2 dipole 0.1 λ over average ground reported 92 − j48 Ω where
  nec2c gives ≈52 + j63 Ω (radiation resistance rose over ground instead of
  dropping). The far-field image path was separately correct, so radiation patterns
  validated while impedance was silently wrong, and the ground-impedance references
  were fnec self-regressions that had pinned the buggy values. Fixed to match the
  far-field image; validated against nec2c via the ground-induced ΔZ (which cancels
  fnec's systematic reactance offset) across four geometries — ΔR sign now correct
  everywhere, near-ground vertical ΔR +18.0 Ω vs nec2c +18.0, horizontal −26 vs −27,
  PEC external-R parity tightened from ≈7 Ω to 0.93 Ω. New gate
  `crates/nec_solver/tests/ground_impedance.rs`; corpus + `ground_diagnostics`
  ground references refreshed to the corrected values. The finite-ground reflection
  still uses a normal-incidence scalar Γ (angle-dependent Fresnel / Sommerfeld remain
  deferred). See `docs/ph9-chk-006-sommerfeld-ground.md`.

- **Closed-loop / T-junction geometries no longer silently return garbage
  (PH9-CHK-002/005 guardrail)** — the conductor-path solve does not handle closed
  loops or degree-3+ (T/Y) junctions, and fnec falls back to the per-wire basis for
  them, whose result is unreliable for the whole geometry. Previously this was only
  warned when the *feed* sat on the junction, so a **loop fed mid-wire produced a
  silent, wrong impedance** (a 1λ square loop reported ≈20 − j1210 Ω vs the nec2c
  truth ≈111 − j146 Ω). A new whole-geometry guard (`classify_unsupported_topology`
  in `geometry.rs`, `warn_if_unsupported_topology` in `solve_session.rs`) now emits a
  class-specific warning (`closed loop` / `T/Y junction`) whenever the topology is
  out of scope, regardless of feed placement. A closed-loop Hallén *solve* was
  prototyped against nec2c but its periodic closure did not validate, so it stays
  deferred (guarded) rather than shipped unvalidated — see
  `docs/ph9-chk-002-general-junction.md`.

### Added

- **PH9-CHK-002 current-source junctions CLI-wired (degree-2)** — the CLI
  current-source path (`solve_current_source_hallen`) now routes degree-2 junctioned
  geometry through the conductor-path solver, so an EX-type-4 current source on a
  bent or connected antenna solves and reports a feedpoint `Z = V/i0` where it
  previously failed fast. Reducible decks keep the per-wire path; degree-3+ (T/Y) and
  closed loops fail fast with a diagnostic. End-to-end gate: a start-to-start split
  dipole's CLI current-source feedpoint `Z` (74.40 + j14.52 Ω) matches the
  voltage-source deck's `Z` (74.41 + j14.52 Ω) to ~2×10⁻⁴
  (`apps/nec-cli/tests/current_source_junction.rs`). This completes the PH9-CHK-002
  degree-2 junction work across all three excitation classes (transmit, plane-wave
  receive, current source). `docs/card-support-matrix.md` EX type 4 updated.

- **PH9-CHK-002 current-source junction solve core (degree-2)** — the conductor-path
  model now also backs the **EX-type-4 current source**, the symmetric-source cousin
  of the plane-wave receive path. Like the voltage delta-gap it needs only one
  homogeneous constant `cos(k·s)` per path (the current is symmetric about the feed)
  plus the unknown port voltage `V`; `solve_hallen_current_source_paths` applies
  `I = 0` at each path's free ends and forces `I[src] = i0`, and
  `build_current_source_shape_paths` builds the unit-voltage source shape over the
  path. Validated by internal consistency: on a start-to-start split dipole and a
  bent inverted-V the current-source `Z = V/i0` matches the voltage-source feedpoint
  impedance to ~2–3×10⁻⁴ (74.40+j14.52 Ω split, 55.51−j11.94 Ω inverted-V), with the
  forced feed current honoured exactly. Self-contained solve core (no CLI/corpus
  churn); CLI wiring is the follow-up increment. See
  `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-002 receive-side junctions CLI-wired (degree-2, plane wave)** — the CLI
  plane-wave receive path (`solve_plane_wave_hallen`) now routes degree-2 junctioned
  geometry through the conductor-path solver, so a **receiving** bent or connected
  antenna (bend, start-to-start / end-to-end split, inverted-V) solves and emits a
  `RECEIVE_PATTERN` where it previously failed fast with `JunctionedGeometryNotSupported`.
  Reducible decks (single wires, collinear chains, parallel arrays) keep the
  validated per-wire path; degree-3+ (T/Y) and closed loops still fail fast.
  End-to-end gate: a start-to-start split dipole's receive sweep shows the correct
  z-dipole shape and matches its own transmit gain pattern by reciprocity to
  0.025 dB (`apps/nec-cli/tests/receive_junction.rs`). `docs/card-support-matrix.md`
  EX type 1 updated. See `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-002 receive-side junction solve core (degree-2, plane wave)** — the
  conductor-path model now backs a *distributed*-excitation solver, so a
  **receiving** bent or connected antenna solves on continuous paths. A plane wave
  induces an asymmetric current, so each conductor path carries **two** homogeneous
  constants (`cos`/`sin`) with `I = 0` at its two free ends only
  (`solve_hallen_planewave_paths`); the forcing sums the incident field over the
  whole path with the traversal-sign + signed-arc-length convention
  (`build_planewave_hallen_paths`). Validated internally: a start-to-start split
  dipole (one arm reversed) reproduces the validated per-wire receive solver to
  machine precision (~1e-11) on the identical mesh, and a bent inverted-V's induced
  feed current tracks its transmit far-field by reciprocity to 1.5 % across a ~8×
  gain range. This is the self-contained solve core (new solver + validation, no
  CLI/corpus churn); routing it into the CLI receive path is the follow-up
  increment. See `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-002 general junction basis (degree-2)** — the Hallén delta-gap solve now
  handles **any degree-2 conductor chain** — bends, start-to-start / end-to-end
  splits, and inverted-V apex feeds — not just collinear splits. `build_conductor_paths`
  walks the wire-endpoint graph into continuous *conductor paths* and the solve
  carries a per-segment traversal sign and signed arc-length, so the homogeneous
  `cos(k·s)` basis stays continuous across the junction with one shared constant per
  path (`build_hallen_rhs_paths` / `solve_hallen_paths`). A λ/2 dipole split at the
  feed now solves 74.41 + j14.52 Ω whether the join is end-to-start or start-to-start
  (was −34 − j1447); a 30°/45°/90° inverted-V matches nec2c's radiation resistance to
  2–4 %. The junction-fed feedpoint warning is suppressed for these now-correct
  cases; degree-3+ (T/Y) junctions, closed loops, and receive-side junctions remain
  guarded (PH9-CHK-005). Zero regression (594 tests). See
  `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-004 near electric and magnetic field (`NE` / `NH` cards)** — fnec can now compute the near
  electric field on a rectangular grid of observation points (`NE I1 NX NY NZ X0
  Y0 Z0 DX DY DZ`), emitting a `NEAR_FIELD` report section. The field is the
  Hertzian-element sum over the solved segment currents (full 1/r, 1/r², 1/r³
  terms). Validated: at 200 λ it is transverse and its magnitude matches the
  independently gain-derived far field to 0.02 %; on a dipole's equatorial axis it
  is axis-polarized with the cross-component vanishing by symmetry. Point-element
  accuracy holds away from the wire surface; very-near-the-wire (extended kernel)
  and spherical grids are out of scope. The `NH` card is the exact magnetic
  companion (`NEAR_H_FIELD` section), validated by the far-field `|E| = η·|H|`
  relationship. `docs/card-support-matrix.md` `NE`/`NH` → Partial.

- **PH9-CHK-004 `PT` print-control** — the `PT` (print-control) card is now applied
  at runtime instead of being parsed-and-ignored: `I1 ≤ −1` suppresses the segment
  current output, `I1 = 0` prints all currents (default), and `I1 ≥ 1` restricts
  the output to tag `I2` and the optional segment range `I3..I4` (last `PT` card
  wins). The former "PT card support is currently deferred" warning is removed;
  `docs/card-support-matrix.md` `PT` → Partial.

### Fixed

- **Benchmark Dashboard CI workflow** — `.github/workflows/benchmark-dashboard.yml`
  had never succeeded (0/30 runs, failing at 0 s). Two causes: (1) the gh-pages
  `index.html` heredocs placed their body at column 0, which terminated the YAML
  `run: |` block scalar and made the whole workflow file invalid; the heredoc
  bodies are now indented into the block scalar (and `$(date)` is captured into a
  shell var so it actually expands). (2) The real-run timing comparison was a hard
  gate, but absolute times for these sub-10 ms corpus decks (plus the GPU-stub
  first-dispatch init cost) are dominated by shared-runner noise, so it flagged a
  "regression" on essentially every run; the real-run comparison is now
  **informational** (a warning annotation + the published dashboard), while the
  deterministic `gate-injection-test` job remains the real gate on the comparison
  logic. Added an explicit `permissions: contents: write` for the gh-pages deploy.

## [0.9.0] — 2026-07-05 — Phase 9 progress: receive patterns, ground gain, junction robustness
### Added

- **Negative-resistance guardrail (PH9-CHK-005)** — a passive antenna cannot have a
  negative input resistance, so a negative `Re(Z)` on the Hallén path now warns that
  the result is unphysical (a junctioned-geometry limitation; see PH9-CHK-002). This
  complements the junction-*fed* warning by catching cases fed *away* from a bad
  junction (e.g. a bent dipole fed mid-arm). Scoped to `--solver hallen` (the pulse
  current-source path has documented negative-`R` values); no valid Hallén corpus
  case trips it.

- **PH9-CHK-002 collinear junction fix** — a straight conductor split across
  several `GW` cards is now solved as one wire. Root cause: fnec's Hallén
  homogeneous solution (`cos(k·s)` + constant) was built per `GW` wire and reset at
  each junction. `merge_collinear_wire_endpoints` merges end-to-start, equal-radius,
  collinear wire chains into one logical conductor for the homogeneous basis; a λ/2
  dipole split at its feed now solves **74.41 + j14.52 Ω** (was −34 − j1447 —
  negative resistance). The merge is a strict no-op for single wires, parallel
  arrays, bends, and stepped-radius junctions, so those are byte-for-byte unchanged.
  Non-collinear junctions (bends, T/Y) remain guarded by PH9-CHK-005.

- **PH9-CHK-005 junction-fed feedpoint guardrail** — feeding a segment that sits
  at a wire junction gives an unphysical impedance in fnec's per-segment `V/I` (a
  half-wave dipole split into two wires and fed at the junction reports
  −34−j1447 Ω instead of the true 74+j14 Ω, because the feed current splits across
  the joined wires). The CLI now **warns** when the driven segment is on a
  junction instead of silently reporting the wrong impedance; the accurate fix is
  PH9-CHK-002. Feeds away from junctions and single-wire geometries are unaffected.

- **PH9-CHK-001 incident-plane-wave receive-pattern sweep** — a plane-wave `EX`
  card with an incidence-angle grid (NTHETA×NPHI, Δθ/Δφ) now produces a
  `RECEIVE_PATTERN` section: the antenna's response vs the wave's arrival
  direction. The per-angle response is the peak induced current — resolved
  empirically to match the transmit gain pattern by reciprocity to <0.01 dB, so no
  arbitrary terminal is needed. `ExCard` gains F4/F5 (Δθ/Δφ). EX types 1/2/3 →
  angle sweeps supported.

- **PH9-CHK-003 absolute gain over finite ground** — the radiation pattern over a
  lossy finite ground now reports **gain** (not directivity): it is scaled by the
  radiation efficiency `η = P_radiated / P_input` (the ground-absorbed power), so
  the reported dBi matches nec2c's absolute gain. Closes the ~1.3 dB
  directivity-vs-gain offset documented in PH8-CHK-006. The normalization constant
  is validated by a lossless free-space dipole (η = 0.9996 ≈ 1); a horizontal
  dipole over average ground matches nec2c's absolute gain to 0.06 dB. Free-space /
  PEC (lossless, η ≈ 1) are unchanged. New public `radiation_efficiency`.

### Docs

- **PH9-CHK-002 junction accuracy diagnosed** — a verified root-cause analysis of
  why junctioned multi-wire feedpoints are mis-solved. A controlled experiment
  (single 52-seg wire → 74.41+j14.52 Ω; the same dipole as two wires → negative
  resistance; *merging* the wire grouping does **not** help) pins the cause to the
  Hallén **homogeneous solution**: the `cos(k·s)` along-wire coordinate resets per
  `GW` wire and the homogeneous constant is independent per wire, so the basis is
  discontinuous across a junction. It is *not* the current-continuity constraint.
  The collinear case of this fix is now implemented (see the PH9-CHK-002 Added entry above); bends/T-junctions remain. See `docs/ph9-chk-002-junction-feed-diagnosis.md`.

- **Phase 9 drafted** (`docs/roadmap.md` "Phase 9: accuracy frontier & scattering
  breadth") — six planned items grounded in the surviving `PRT-*` gaps and the
  Phase 8 frontier deferrals: incidence-angle sweeps + receive pattern, junctioned
  multi-wire receive solves, absolute gain over lossy ground, PT + full RP output
  modes, a difficult-geometry accuracy corpus, and a first Sommerfeld/buried
  near-ground increment. A draft for review; first-frontier priority is a product
  decision.

### Fixed

- **`RP` card XNDA field** — the radiation-pattern card parser now accepts the
  canonical 8-field NEC form (`RP mode Nθ Nφ XNDA θ0 φ0 Δθ Δφ`) in addition to
  fnec's legacy 7-field form. Previously a standard 8-field `RP` card mis-parsed
  θ0 (it read the XNDA/I4 value as θ0), so real 4nec2 pattern decks produced an
  all-null pattern. Distinguished by field count; XNDA does not affect the angle
  grid.

## [0.8.0] — 2026-07-04 — Phase 8 complete: mainstream deck portability
### Added

- **PH8-CHK-005 lossy transmission line** — `TL` cards with `tl_type != 0` now
  stamp a lossy line, `Z0·coth(γℓ)` / `Z0·csch(γℓ)` with complex `γℓ = αℓ + jβℓ`
  (velocity factor 1, `F3` = matched-line loss in dB). Reduces exactly to the
  lossless `−jZ0·cot/csc` at 0 dB. Validated: lossless limit <1e-9, attenuation
  monotone with loss, high-loss input impedance → Z0. **Completes the Phase 8
  checklist (PH8-CHK-001..006).** `docs/card-support-matrix.md` `TL other` →
  Partial.

- **PH8-CHK-006 radiation pattern over finite ground** — the far-field over a
  finite (imperfect) ground now uses the Fresnel reflection-coefficient
  approximation instead of the free-space pattern (only PEC ground had an image
  before). A horizontal/vertical antenna over real earth now shows the correct
  ground lobe and horizon null. Validated: PEC limit matches to <0.05 dB; the
  pattern shape matches nec2c to 0.053 dB (horizontal dipole over average ground).
  fnec reports directivity; the ~1.3 dB offset vs nec2c gain (ground-loss
  efficiency) is documented.

- **PH8-CHK-003 EX type 5 (voltage source)** — EX type 5 (voltage source,
  current-slope discontinuity) now solves: fnec models it via its applied-field
  method, so the feedpoint impedance equals type 0's, on both `--solver hallen`
  and `--solver pulse`. This completes the EX-source family (types 0–5).
  Deck-portability (CP-003): type-5 decks run instead of failing. NEC's separate
  current-slope numerics (~6% different) are a documented non-goal.
  `docs/card-support-matrix.md` EX type 5 → Partial.

- **PH8-CHK-001/002 non-junctioned multi-wire** — incident plane waves and
  current sources now solve on **one or more straight, non-junctioned wires**
  (e.g. a parallel dipole array), not just a single wire. The plane-wave Hallén
  forcing is per-wire (own axis, own along-wire coordinate, same-wire kernel
  sum). Validated: each wire's induced-current shape matches nec2c (~10%); a
  symmetric-broadside wave induces equal currents on two parallel wires (5e-11);
  a two-wire current-source port impedance matches the voltage source. Junctioned
  geometry fails fast.
- **PH8-CHK-002 elliptic plane waves (EX types 2/3)** — right- and left-hand
  elliptic incident plane waves now solve on `--solver hallen`. The incident
  field uses a complex polarization vector (`ê = û_maj + j·sense·AR·û_minor`,
  axial ratio from EX F6, handedness from the type). Validated: on a z-wire (or
  axial ratio 0) elliptic reduces exactly to linear; on a tilted wire the induced
  currents match nec2c's elliptic reference (5.4% shape). `ExCard` gains a
  `polarization_ratio` field. The legacy `--ex3-i4-mode` flag is now an obsolete
  no-op (type 3 is a plane wave). EX types 2/3 → Partial.
- **PH8-CHK-004 NT two-port network** — user-runnable: `NT` cards are stamped
  into the Z matrix by converting their admittance parameters to impedance
  parameters (`[Z]=[Y]⁻¹`), mirroring the TL stamp. A well-formed reciprocal NT
  reproduces the equivalent TL feedpoint impedance end to end
  (`dipole-nt-tl-equiv-freesp-51seg`, matches to ~1e-5 Ω). The blanket "NT
  deferred" warning is removed; malformed / singular-admittance / missing-endpoint
  cards warn and are skipped. `docs/card-support-matrix.md` NT → Partial.
- **PH8-CHK-001 current source (NEC2 EX type 4)** — user-runnable end to end:
  `solve_hallen_current_source` treats the port voltage as an unknown and forces
  `I[src]=i0`, the exact dual of the delta-gap voltage source; validated by
  impedance-consistency (current-source Z equals voltage-source Z to 2×10⁻⁴). The
  CLI routes single-straight-wire type-4 decks on `--solver hallen` and reports
  `FEEDPOINTS Z=V/i0`; the `dipole-ex4` corpus case validates the impedance.
  Multi-wire geometry and non-Hallén solvers fail fast.
  `docs/card-support-matrix.md` EX type 4 → Partial.
- **Project traceability layer** (`docs/project/`): a consolidated
  requirement → design → implementation → tests → results matrix with a
  per-push maintenance rule (#256).
- **PH8-CHK-002 CLI wiring** — incident plane-wave decks are now user-runnable:
  `--solver hallen` on a single straight wire with a linear plane wave (EX type 1)
  produces a receiving-antenna solve — induced `CURRENTS`, no feedpoint impedance.
  Elliptic polarization (types 2/3), multi-wire geometry, and non-Hallén solvers
  fail fast with actionable diagnostics. `docs/card-support-matrix.md` EX type 1
  → Partial.
- **PH8-CHK-002 solve core** — incident plane-wave Hallén solve:
  `nec_solver::planewave` builds the plane-wave forcing RHS (tangential incident
  field integrated with the delta-gap Hallén normalization), and
  `solve_hallen_planewave` solves it with a two-DOF (cos+sin) homogeneous system
  — the freedom classical Hallén needs for an asymmetric receive current. The
  shared delta-gap `solve_hallen` is untouched. Validated: nec2c induced-current
  shape parity 4.3%, broadside symmetry exact, Rayleigh–Carson reciprocity vs the
  validated transmit far-field exact. Not yet wired into the CLI (next
  increment).
- **PH8-CHK-002 code foundation** — NEC2 EX-type alignment in code: an
  `ExcitationKind` classifier (single source of the NEC2 0–5 numbering),
  `ExCard.polarization_deg` (plane-wave polarization field F3, read by the
  parser), and a NEC2-category-accurate reject diagnostic (e.g. *"incident
  plane wave, linear polarization (type 1) … is not yet supported"*). EX types
  1–5 still fail fast — the plane-wave/current-source solves are later
  increments — so no corpus contract changed. `docs/card-support-matrix.md` EX
  rows corrected to NEC2 numbering.

### Changed

- **Dependency hygiene**: documented, scoped exception for two `quick-xml` DoS
  advisories (RUSTSEC-2026-0194/0195) in `.cargo/audit.toml` + `deny.toml` —
  build-time-only Wayland proc-macro path, root fix blocked upstream. Revisit
  when wayland-scanner ships `quick-xml >= 0.41`.

## [0.7.0] — 2026-06-27 — Phase 7 complete: GPU productionization
### Added

- **PH7-CHK-006 — native ROCm/SYCL backend: dated deferral**. `docs/multi-vendor-gpu.md`
  records a verified, dated deferral: the AMD target (Renoir `gfx90c` APU) is
  outside AMD's ROCm support matrix, no ROCm/HIP/OpenCL/SYCL toolchain is present,
  and a native backend would duplicate kernels for no correctness gain over the
  already-validated RADV Vulkan path. Concrete blockers + revisit trigger and the
  backend matrix updated; corrected a stale "GPU dispatch deferred" note now that
  PH7-CHK-003/004 dispatch real kernels.

- **PH7-CHK-005 — real discrete-GPU benchmark evidence**: harness
  `apps/nec-cli/examples/gpu_crossover.rs` measures the Z-matrix-fill and RP
  kernels against CPU on a real AMD GPU (`RADV RENOIR`, Vulkan). Artifact
  `benchmarks/real-gpu-crossover.json`; crossover documented in `docs/benchmarks.md`
  (Z-fill kernel-only: GPU beats CPU below 32 segments, up to ~240× at 1536;
  RP wall-clock 1.5–1.8× faster). Refreshes the retired `FNEC_ACCEL_STUB_GPU`
  references in `docs/benchmarks.md`. See `docs/ph7-chk-005-real-gpu-benchmark.md`.

- **PH7-CHK-002 — in-process GPU microbenchmark**: `nec_accel::microbench_zmatrix_dispatch`
  pays the wgpu device-initialization cost once and times many reused kernel
  dispatches, so per-dispatch time is isolated from device-init (which the
  across-process G5 gate cannot separate). Returns `GpuMicrobench { device_init_us,
  dispatch_min_us, dispatch_median_us, .. }`. Artifact schema documents the
  optional `gpu_microbench` object (and corrects the retired `FNEC_ACCEL_STUB_GPU`
  reference). Measured ~61 ms device-init vs ~0.27 ms dispatch; non-flaky over 10
  runs. See `docs/ph7-chk-002-gpu-microbenchmark.md`.

- **PH7-CHK-004 — distributed GPU execution**: `--exec gpu` is wired through the
  `nec_worker` SSH pool. New `WorkerSolverConfig.exec` request and
  `TaskResult.exec_used` report fields (serde-default for wire back-compat);
  `solve_deck_at_frequency_with_exec` dispatches the GPU-resident solve
  (PH7-CHK-003) on a node with a wgpu adapter for the supported deck class, and
  falls back to the CPU solve otherwise. `nec_worker` now depends on `nec_accel`
  (wgpu). See `docs/ph7-chk-004-distributed-gpu-execution.md`.

- **PH7-CHK-003 — GPU-resident Hallén solve**: `solve_hallen_gpu_resident`
  (`crates/nec_accel`, `shaders/hallen_normal_solve.wgsl`) fills the Z-matrix and
  solves the regularized normal-equations system entirely on the GPU — Jacobi
  equilibration + complex LU (partial pivoting) + Björck least-squares refinement
  — returning only the solution vector (the N×N matrix never leaves the device).
  Wired into CLI `--exec gpu` for the supported Hallén class (free-space, no
  LD/TL). Matches the f64 CPU solve to ~0.01 Ω on the reference dipole. f32
  precision; the f64 CPU solve stays the corpus-gate reference. See
  `docs/ph7-chk-003-gpu-resident-solve.md`.

### Changed

- **PH7-CHK-001 — retired the GPU CPU-emulation scaffold**: removed every code path
  that reported CPU compute as GPU work. `nec_accel::gpu_kernels` is now documented
  and named as the **CPU reference** far-field kernel (parity baseline for the wgpu
  shaders); `compute_hallen_fr_*_stub` renamed to `*_cpu`. Removed the
  `FNEC_ACCEL_STUB_GPU` env hack, `ExecutionPath::GpuStubEmulation`,
  `execute_frequency_point`, the dead `HallenRhsGpuKernel`/`PocklingtonMatrixGpuKernel`
  structs, and the "accelerator stub backend … CPU emulation" warnings. See
  `docs/ph7-chk-001-gpu-stub-retirement.md`.

### Removed

- **`--gpu-fr` CLI flag**: it only ran a CPU computation labelled as GPU. Superseded by
  `--exec gpu`, which dispatches the real wgpu RP / Z-matrix-fill kernels.

## [0.6.0] — 2026-05-05
### Added

- **Phase 6 complete** — all seven PH6-CHK items done (CI benchmark dashboard, NEC-5 frontier decision, sinusoidal-basis EFIE, multi-vendor GPU validation, distributed execution design, SSH-backed worker deployment, SHA-256 result cache).
- **`nec_worker` crate**: new library crate implementing the distributed worker protocol — `TaskMessage`/`TaskResult` JSON-lines protocol, `HostsConfig` TOML, per-node `CapabilityCache`, `solve_deck_at_frequency()` Hallén pipeline, `run_worker_stdio()` event loop, and `LocalWorkerHandle` subprocess controller.
- **`fnec worker --stdio` subcommand**: worker node mode added to `nec-cli`; spawns a JSON-lines solve loop on stdin/stdout for SSH-pipe transport.
- **SHA-256 result cache (`ResultCache`)**: deterministic cache keyed on `hash(deck + solver_config + freq_hz)`; FIFO-bounded capacity; hit/miss/invalidation contract tests; 5-point sweep reuse demonstrated.
- **CI benchmark dashboard (PH6-CHK-001)**: GitHub Actions workflow publishing benchmark JSON artifacts; regression delta threshold enforced.
- **NEC-5 frontier decision doc** (`docs/nec5-frontier.md`): explicit wire-only continuation decision; ≥3 corpus expansion cases mapped to PH6N5-* rows.
- **Sinusoidal-basis EFIE (PH6-CHK-003)**: piecewise-sinusoidal matrix assembly in `nec_solver`; EXPERIMENTAL warning retired.
- **Multi-vendor GPU doc** (`docs/multi-vendor-gpu.md`): Vulkan/Metal/DX12/OpenCL backend matrix; AMD validation; ROCm/SYCL deferred path documented.
- **Distributed execution design doc** (`docs/distributed-execution-design.md`): SSH stdio transport, ed25519 authN, worker contract, frequency-point work-split, result-cache design.
- **Worker deployment guide** (`docs/worker-deployment.md`): per-node SSH key setup, `hosts.toml` reference, wire protocol examples, troubleshooting.

## [0.5.0] — 2026-05-04
### Added

- **Phase 2 complete** — all eight PH2-CHK items done (ground models, buried-wire guardrails, source/load/network semantics, report/table parity, corpus truth expansion, geometry diagnostics, NEC-5 validation matrix, scriptability preservation).
- **Phase 5 complete** — all seven PH5-CHK GPU acceleration items done (G1–G7 gates: architecture decision, wgpu scaffold, RP WGSL kernel, CLI `--exec gpu` wiring, CPU-vs-GPU benchmark gate, Z-matrix fill WGSL kernel, full GPU Hallén solve path).

## [0.4.0] — 2026-05-02
### Added

- **PH5-CHK-007 (Full GPU Hallén solve path — gate G7)**: `--exec gpu` now uses `fill_zmatrix_wgpu` (from PH5-CHK-006) to fill the Hallén A-matrix on the GPU for free-space and deferred-ground decks, then feeds the result to the existing CPU LU (`solve_hallen`). Ground-augmented models (PEC, finite ground) retain the CPU fill path. `ZMatrix::from_flat` constructor added to `nec_solver` for building a `ZMatrix` from a flat row-major `Vec<Complex64>`. GPU path falls back to CPU with a `stderr` warning when no wgpu adapter is available. New gate G7 end-to-end test `crates/nec_accel/tests/gpu_hallen_solve.rs`: builds a 51-segment dipole at 14 MHz, fills Z on GPU, solves with CPU Hallén, checks feedpoint impedance within ±2 Ω of all-CPU reference; achieved ΔR=0.000 Ω, ΔX=0.000 Ω (GPU f32 precision is sufficient for accurate solve).

- **PH5-CHK-006 (GPU Z-matrix fill WGSL kernel — gate G6)**: New `crates/nec_accel/src/shaders/zmatrix_fill.wgsl` — WGSL compute shader that fills the N×N Hallén A-matrix; each thread computes one element Z[i,j]. Off-diagonal elements use 8-point GL with reduced kernel; self elements use 4-point GL smooth part + analytic log singularity subtraction (identical algorithm to CPU `assemble_z_matrix`). New public async `fill_zmatrix_wgpu(segments, freq_hz)` in `wgpu_device.rs` packs f64 segment data (including radius) into a `GpuSegmentZ` buffer, dispatches `ceil(N²/64)` workgroups, and reads back `Vec<ZElem>` (re, im f32 pairs, row-major). New `ZSegmentInput` type in `nec_accel` avoids circular dependency with `nec_solver`. New parity test `crates/nec_accel/tests/gpu_zmatrix_parity.rs`: builds a 51-segment dipole, compares GPU vs CPU Z-matrix with max relative error ≤ 1×10⁻⁴; passes vacuously when no GPU adapter is available. Achieved max rel err = 2.12×10⁻⁶ on local hardware.

- **PH5-CHK-004 (CLI `--exec gpu` wired to wgpu RP kernel — gate G4)**: `--exec gpu` now dispatches the RP far-field computation through the real wgpu compute kernel (`run_rp_farfield_batch_wgpu`) instead of the CPU stub. New `run_rp_farfield_batch_wgpu()` in `wgpu_device.rs` reuses the wgpu device, compiled pipeline, and segment/current buffers across all observation points — only the 16-byte uniforms buffer is updated per point via `queue.write_buffer`. When no adapter is available a stderr warning is emitted and the code gracefully falls back to the CPU path. New `pub fn integrate_radiated_power()` exported from `nec_solver` computes the total radiated power normalisation integral needed to convert GPU `U_θ/U_φ` outputs to gain (dBi). `nec-cli` now depends on `nec_accel` with `features = ["wgpu"]` and `pollster` for synchronous dispatch. New integration test `gpu_rp_exec.rs`: two tests — gain parity check (≤0.5 dBi) vs CPU reference, and exec diag field assertion. All `cargo test -p nec-cli` tests pass.

- **PH5-CHK-003 (RP WGSL kernel — milestone gate G3)**: New `crates/nec_accel/src/shaders/rp_farfield.wgsl` — a WGSL compute shader that computes far-field radiation intensity components `(U_θ, U_φ)` for one observation direction by summing over all wire segments (matches the algorithm in `gpu_kernels::far_field_components` exactly). New public async function `run_rp_farfield_wgpu()` in `wgpu_device.rs` dispatches the shader end-to-end: packs f64 segment/current data into f32 GPU buffers, sets up bind group and pipeline from the embedded shader, dispatches one workgroup, and reads back `RpGpuResult { u_theta, u_phi }`. `bytemuck = "1"` added to workspace and `nec_accel` (wgpu-feature-gated) for zero-copy buffer packing. New parity test `wgpu_rp_farfield_parity_vs_cpu_stub` asserts GPU gains match CPU stub within 0.5 dBi across 5 observation directions on a 3-segment dipole; vacuously passes when no adapter is available (headless CI safe). All 15 `nec_accel` tests pass with `--features wgpu`.

- **PH5-CHK-002 (wgpu scaffold — milestone gate G2)**: Added `wgpu = "29"` to `nec_accel` behind `--features wgpu` flag. New `crates/nec_accel/src/wgpu_device.rs`: `enumerate_compute_adapters()` lists all runtime-visible adapters; `run_noop_compute_pipeline()` compiles and dispatches a trivial WGSL no-op shader end-to-end, returning `NoOpPipelineResult::Success` or `NoOpPipelineResult::NoAdapterAvailable` (graceful on headless CI). `pollster` added as dev-dependency for blocking async tests. Two new tests in `nec_accel`: adapter enumeration (no panic) and no-op pipeline (success or graceful skip). Baseline (no-feature) build unchanged.

- **PH5-CHK-001 (GPU architecture decision)**: New `docs/gpu-arch.md` locking the Phase 5 GPU acceleration architecture: wgpu (Rust-native, Vulkan/Metal/DX12/OpenCL) chosen as primary API; WGSL as compute shader language; RP far-field gain computation chosen as first-offload candidate (embarrassingly parallel, existing stub baseline in `nec_accel::gpu_kernels`); real-hardware validation minimum defined (G3 gate on workstation + Pi5 before matrix-fill work); 7-gate milestone sequence G1–G7 defined; CPU fallback contract specified. Resolves GAP-007. Phase 5 checklist PH5-CHK-001…007 added to `docs/roadmap.md`.

- **PH4-CHK-007 (Phase 5 entry criteria)**: New `docs/phase5-entry-criteria.md` defining 5 measurable go/no-go criteria before GPU acceleration work begins: (1) CPU baseline benchmarks locked on 2+ targets, (2) solver tolerance validated on 4+ corpus decks, (3) Phase 4 plugin surface (EP-1…EP-4) declared stable, (4) `cargo deny` policy clean, (5) Phase 4 checklist complete. All 5 criteria are met as of 2026-05-03. References `docs/benchmarks.md` baseline tables and `docs/requirements.md` tolerance matrix. Passes frontmatter CI gate.

- **PH4-CHK-006 (automation guide)**: New `docs/automation-guide.md` documenting all automation surfaces: JSON output consumption, batch sweep patterns, `--vars` template workflows, resonance targeting, optimizer loop patterns (golden-section and scipy), and the `fnec_py` Python binding. New `examples/optimize_swr.py`: a self-contained end-to-end script (stdlib only) that drives `fnec --output-format json` to find the dipole half-length minimising SWR at 14.2 MHz. Runs end-to-end in ~18 solver calls. Fixed pre-existing frontmatter failures in `docs/json-output-schema.md` and `docs/python-bindings.md` (wrong key names — now compliant with CI gate).

- **PH4-CHK-005 (EP-4 DeckValidator)**: Added `DeckValidator` trait, `ValidationDiagnostic` struct, `DiagnosticLevel` enum, and `run_validators()` helper to `nec_model`. Validators receive a read-only `&NecDeck` and return a `Vec<ValidationDiagnostic>`; `run_validators` aggregates results across all validators without short-circuiting. CLI wires in a built-in `NoExCardValidator` (warning-level) on every solve path, emitting `warning: [validator] …` to stderr. Error-level diagnostics produce a non-zero exit code. `docs/plugin-api-design.md` updated: EP-4 section added, pipeline diagram updated, EP-4 removed from the "Planned" table. Tests: 7 unit tests in `crates/nec_model`, 2 doctests (`DeckPostProcessor`, `DeckValidator`), 4 integration tests in `apps/nec-cli/tests/deck_validator.rs`.

- **PH4-CHK-004 (Python bindings)**: New `bindings/fnec_py/` crate (PyO3 0.23, cdylib). Exposes `solve_deck_str(deck: str) -> dict` and `sweep_deck_str(deck: str) -> list[dict]` returning `{freq_mhz, tag, seg, z_re, z_im, z_abs, z_arg_deg}`. Uses Hallen solver internally. Build: `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop` from `bindings/fnec_py/`. 8 smoke tests in `bindings/fnec_py/tests/test_smoke.py`. Build instructions in `docs/python-bindings.md`.

- **PH4-CHK-003 (`--output-format json`)**: `fnec` now accepts `--output-format json` on all solve/sweep paths. Output is a JSON array — one record per frequency point — with fields `freq_mhz`, `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg`. Text output unchanged when flag is omitted. Schema locked in `docs/json-output-schema.md` (schema v1). 5 contract tests in `apps/nec-cli/tests/json_output_contract.rs`.

- **PH4-CHK-002 (EP-3 custom report sections)**: Added `ReportSection` trait and `render_text_report_with_sections()` to `nec_report`. Callers pass a `&[&dyn ReportSection]` slice; each section's `render()` output is appended after the standard report sections. Two doctests (`ImpedanceSummary`, `Banner`) and 4 unit tests (identity, single-section append, multi-section ordering, `PeakImpedanceSection` worked example). `docs/plugin-api-design.md` updated with EP-3 section description, revised pipeline diagram, and updated future-EP table (EP-4/5/6). `cargo test -p nec_report`: 11 unit tests + 3 doctests.

- **PH4-CHK-001 (dependency policy + cargo-deny)**: Authored `docs/dependency-policy.md` resolving BLK-005. Covers the SPDX allowlist (13 identifiers), deny-list (GPL-2.0-only, AGPL, SSPL, BUSL, proprietary), GPLv2 vs. GPLv3 compatibility rules, exception request process, duplicate-version and source policies, and tooling instructions. Added `deny.toml` with `cargo-deny` v2 schema: unconditional allowlist, `self_cell` exception (Apache-2.0 option), advisory deny, duplicate-version warn, sources deny for unknown registries and git deps. `cargo deny check licenses` passes cleanly. BLK-005 marked resolved. Fixed stale SBOM format flag in `docs/steering.md` (`spdx-json` → `spdx_json_2_3`). Added Phase 4 implementation checklist (PH4-CHK-001..007) to `docs/roadmap.md`.

- **PH3-CHK-012 (Phase 3 usability benchmark)**: Authored `docs/usability-benchmark-ph3.md` satisfying all three Phase 3 usability acceptance minima. Benchmark 1 records the 5-point frequency sweep from a blank `fnec-gui` project in exactly **7 explicit actions** with a step-by-step table. Benchmark 2 records an edit-run-inspect workflow comparison against xnec2c: fnec-gui completes in 4 steps (~15 s) vs. xnec2c's 5 steps (~22 s). The document includes the acceptance-minima checklist with all items ticked.

- **PH3-CHK-011 (nec-gui pattern slice + current-distribution views)**: Added two new tabs to `fnec-gui`: Pattern and Currents. The Pattern tab computes an elevation-plane (fixed φ) radiation-pattern slice in 5° θ steps (37 points) using the existing `nec_solver::compute_radiation_pattern` API; the Currents tab shows per-segment current magnitudes as a text bar chart. Implementation: `solve.rs` gains `PatternPoint`, `CurrentPoint`, `pattern_slice_deck_str/path`, `current_distribution_deck_str/path`, and a shared `solve_for_currents()` helper that builds geometry once. `app_state.rs` extended with `ActiveTab::Pattern/Currents`, `PatternPhase`, `CurrentsPhase`, `PatternDisplayRow`, `CurrentDisplayBar` (data-to-plot mapping structs), `can_run_pattern()`, `can_run_currents()`, `pattern_phi()`, `pattern_display_rows()`, `current_display_bars()`, `pattern_status_text()`, `currents_status_text()`. `main.rs` updated with four-tab bar, `pattern_view()`, `currents_view()`, `pattern_table()`, `currents_bars()` helpers. Added 20 new headless tests (6 pattern state machine, 3 currents state machine, 4 data-to-plot mapping, 4 pattern pipeline, 3 current pipeline) for a total of **47 smoke tests**.

- **PH3-CHK-010 (nec-gui sweep views)**: Added frequency-range sweep setup and result inspection views to `fnec-gui`. The GUI gains a Solve/Sweep tab bar switching between the existing single-frequency panel and a new sweep panel. The sweep panel provides Start/End/Step (MHz) text inputs, a Run Sweep button, a progress/status line, and a sortable four-column result table (Freq, Z_re, Z_im, |Z|). Column headers are clickable sort buttons with ascending/descending toggle indicators. Implementation: `app_state.rs` extended with `ActiveTab`, `SweepPhase`, `SweepSortCol`, `SweepSetup` fields, new `Message` variants (`TabSelected`, `SweepStartChanged`, `SweepEndChanged`, `SweepStepChanged`, `RunSweep`, `SweepComplete`, `SweepSortBy`), `can_sweep()`, `sweep_params()`, `sorted_sweep_rows()`, `sweep_status_text()`. `solve.rs` gains `SweepPoint` struct and `sweep_deck_str` / `sweep_deck_path` functions that build geometry once and iterate the impedance-matrix solve over each frequency. `main.rs` updated with tab bar, `sweep_view()`, `sweep_result_table()`, `sweep_row()` helpers. Added 14 new headless tests to `gui_smoke.rs` covering sweep state machine (8 tests) and sweep pipeline (5 tests), for a total of 27 smoke tests.

- **PH3-CHK-009 (nec-gui iced desktop window)**: Implemented the `fnec-gui` desktop frontend using `iced` 0.13. The binary presents a dark-themed window with a deck path text input, a Solve button, and a result panel showing frequency, Z_re, Z_im, and |Z|. The solve pipeline runs asynchronously via `Task::perform`. Implementation split: `apps/nec-gui/src/lib.rs` + `app_state.rs` (state machine — no iced dep, fully headless-testable) + `solve.rs` (Hallen solve wrapper calling `nec_solver` directly). Added 13 headless smoke tests in `apps/nec-gui/tests/gui_smoke.rs` covering state machine transitions (8 tests) and solve pipeline correctness (5 tests). Added `.github/workflows/gui-smoke.yml` CI gate running `cargo test -p nec-gui --test gui_smoke`.
### Added

- **PH3-CHK-008 (resonance-targeting helper)**: Added `fnec sweep --resonance <file.nec.toml>` subcommand that binary-searches one template variable to find the feedpoint reactance closest to a target (typically 0 Ω for series resonance). The `.nec.toml` file embeds both a `[search]` table (variable name, lo/hi bounds, target reactance, tolerance, max iterations) and a `[deck]` table containing the NEC template string. Implementation: `apps/nec-cli/src/resonance_search.rs` (`ResonanceFile` TOML struct, `bisect()` function, `print_result()`). Integrates with the template engine from PH3-CHK-007 and re-runs the full geometry/solve pipeline for each probe point. Added `examples/resonance-search.nec.toml` worked example (14.2 MHz dipole resonance search); added 3 contract tests in `apps/nec-cli/tests/resonance_contract.rs` (convergence, unbounded-range error, missing-flag usage error).

- **PH3-CHK-007 (variable-substitution engine)**: Added `nec_parser::template` module with a `substitute()` function that replaces `$VAR` tokens in NEC deck strings from a `HashMap<String, String>`. `$$` produces a literal `$`; undefined tokens return a `TemplateError` with the variable name and 1-based line number. CLI: `--vars <file>` flag loads a flat TOML or JSON key→value map and applies substitution before parsing. Added `apps/nec-cli/src/vars_config.rs` (TOML via `toml` crate; JSON via minimal hand-rolled parser). Added 5 contract tests in `apps/nec-cli/tests/template_contract.rs`. Corpus example: `corpus/variable-dipole.nec` (template) + `corpus/dipole-vars.toml` (vars). `--vars` documented in `docs/cli-guide.md` synopsis and options table.

- **PH3-CHK-006 (`--sweep-config` CLI flag)**: Added `--sweep-config <file.toml>` flag to the `fnec` binary. A TOML sweep-config file specifies a frequency list as either a linear range (`start_mhz`, `end_mhz`, `step_mhz`) or an explicit point list (`points_mhz = [...]`). When supplied, the sweep-config frequencies replace those derived from the deck's `FR` card; the full solve pipeline runs once per point and emits one structured output block per frequency on stdout. Implementation: `apps/nec-cli/src/sweep_config.rs` (TOML reader + validation); `apps/nec-cli/Cargo.toml` gains `serde` and `toml` workspace deps; `apps/nec-cli/tests/sweep_contract.rs` adds 5 contract tests (single-point explicit, multi-point explicit, range point-count, ordering stability, machine-parseability); `examples/sweep-spec.toml` provides a range-based reference example.

- **PH3-CHK-005 (run history API)**: Extended `nec_project` with `RunHistory` (transparent `Vec<RunRecord>`), `RunRecord` (ISO 8601 timestamp, `SolverConfig` snapshot, `ResultSummary`), and `ResultSummary` (impedance Re/Im, optional peak gain dBi, sweep point count). `ProjectFile` gains `run_count()`, `last_run()`, and `run_by_index()` query methods plus `RunHistory::push`. History is absent from TOML when empty; `peak_gain_dbi` is omitted when `None`. 5 history tests added (13 integration + 1 doctest total).

- **PH3-CHK-004 (nec_project TOML format)**: Implemented `ProjectFile`, `SolverConfig`, and `NamedRun` structs with serde/toml round-trip in `crates/nec_project/src/lib.rs`. Public API: `ProjectFile::from_toml` / `to_toml`; `ProjectError` with version-guard (`UnsupportedVersion`). 8 integration tests + 1 doctest in `crates/nec_project/tests/project_roundtrip.rs`. Project TOML format documented in `docs/project-format.md`.

- **PH3-CHK-003 (plugin API design)**: Added `docs/plugin-api-design.md` covering the extension surface, safety model (no network/filesystem/FFI through the trait interface), pipeline diagram, and future EP-3..5 scope. Implemented two working extension points: `DeckPostProcessor` trait (EP-1) in `crates/nec_model/src/lib.rs` (called after parse, before geometry build) and `ResultFilter` trait (EP-2) in `crates/nec_report/src/lib.rs` (called after solve, before report rendering). Both are exercised by doctests. BLK-004 updated to resolved.

- **PH3-CHK-002 (contributing guide)**: Added `docs/contributing.md` covering build workflow, pre-push sequence (`cargo fmt` → `cargo check` → `cargo test`), branch conventions, PR process, corpus-gate requirements, documentation frontmatter rules, and architecture orientation for new contributors. Added contributor orientation cross-references to `docs/architecture.md` and `docs/design.md`. The `validate-doc-frontmatter` CI gate picks up the new file automatically via its existing `docs/*.md` glob.

- **PH3-CHK-001 (card-status index)**: Added `## PH3-CHK-001 complete card status index` section to `docs/nec4-support.md` with a 25-row flat table listing every known NEC-2/NEC-4 mnemonic, its parser status (`recognized` / `unknown`), and functional status. Documents the GM/GR gap (geometry builder implemented but parser not yet wired). `par001_card_status_table_complete` test in `apps/nec-cli/tests/corpus_validation.rs` enforces all 12 parser-recognized mnemonics and 3 out-of-scope entries are present in CI.

- **Non-collinear multi-wire Hallen support (Phase 2)**: The Hallen solver now handles junctioned and non-collinear multi-wire topologies (e.g. `dipole-loaded` top-hat geometry, inverted-V, Yagi with passive elements) via a segmented hybrid reformulation:
  - `build_hallen_rhs` now computes per-wire local cos(k·s) homogeneous vectors using each wire's own midpoint as s=0, replacing the old global s-axis.
  - Passive (non-driven) wires receive rhs=0; all EX cards contribute to the source map (multi-source support).
  - `detect_wire_junctions()` in `geometry.rs` identifies shared wire endpoints; `solve_hallen` enforces KCL continuity rows for junction segments instead of the default I=0 endpoint condition.
  - `--allow-noncollinear-hallen` flag is now silently accepted (no-op) rather than deferred; non-collinear geometries are supported by default.
  - `dipole-loaded` corpus gate now passes: Z ≈ 12.39 − j918 Ω (external NEC2 reference: 13.46 − j896 Ω).
  - References for TL-coupled multi-dipole cases and Yagi 5-element case updated to reflect correct passive-wire rhs=0 behavior.

### Changed

- Extracted geometry validation helpers (`sinusoidal_a4_topology_supported`, `segment_intersection_error`, `source_risk_geometry_error`, `buried_wire_geometry_error`, and private math/graph helpers) into `apps/nec-cli/src/geometry_validation.rs`, and extracted all warning functions into `apps/nec-cli/src/warnings.rs`. `main.rs` is now reduced to frontend wiring, enums/constants, bench-emit helpers, and `fn main()`.
- Extracted per-frequency solve-session logic from `apps/nec-cli/src/main.rs` into a new `apps/nec-cli/src/solve_session.rs` module: all math helpers (`l2_norm`, `matrix_diagonal_spread`, `residual_zi_minus_v`, `residual_hallen`), pulse-source constraint helpers, report builders (`build_feedpoint_rows`, `build_source_rows`, `build_load_rows`), frequency/dispatch helpers (`frequencies_from_fr`, `build_hybrid_lane_plan`), all four structs (`FrequencySolveResult`, `SweepPointSummary`, `PulseCurrentSourceConstraint`, `HybridLanePlan`), and `solve_frequency_point` now live in `solve_session`. The function gains an explicit `sinusoidal_topology_supported: bool` parameter, computed once in `main()` before the solve closure, replacing the internal call to `sinusoidal_a4_topology_supported` inside the solve path.

- Continued CLI decomposition by extracting execution-profile policy logic (4nec2 drop-in detection/steering and startup auto-probe mode selection) from `apps/nec-cli/src/main.rs` into `apps/nec-cli/src/exec_profile.rs`.
- Started three accepted review follow-ups: parser fuzz scaffolding now exists under `fuzz/`, CLI argument parsing/usage text now lives in `apps/nec-cli/src/cli_args.rs`, and `nec_solver` now carries a first property-based Hallen reciprocity invariant test.
- Review follow-up triage now assigns owners and concrete closure criteria for the remaining GAP items, adds measurable Phase 3 usability minima, documents experimental residual budgets and the scoped GN0/GN2 finite-ground validity envelope, and starts documenting crate-level public surfaces for `nec_report` and `nec_project`.
- Report contract coverage now locks combined sweep-plus-operator-table ordering on stdout: multi-frequency runs with `LD` cards must emit one full per-frequency block in `FEEDPOINTS -> SOURCES -> LOADS -> CURRENTS` order before the final `SWEEP_POINTS` summary.
- Added a supported low above-ground GN2 near-ground corpus contract (`dipole-gn2-near-ground-51seg`) and tightened PH2-CHK-002 docs/tests so supported near-ground coverage is distinguished from buried active-ground fail-fast guardrails.
- Geometry diagnostics now also fail fast for source-risk tiny segments: `EX` requests on `L/r < 2` emit an actionable deferred-class error before solve.
- GN type 0 is now active as a simple finite-ground model in Hallen impedance assembly (complex Fresnel-style image scaling from EPSE/SIG) instead of the prior deferred free-space fallback warning path.
- Phase 2 current/phase corpus coverage now includes both `dipole-freesp-51seg` and `dipole-ground-51seg`, so CI locks representative free-space and PEC-ground current magnitude/phase samples instead of only the base dipole case.
- EX type 1 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 2 is now accepted as a staged portability fallback: the CLI warns that incident-plane-wave semantics are still pending, and current runtime behavior treats EX type 2 like EX type 0 until a dedicated implementation lands.
- EX type 4 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 5 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- TL `NSEG>1` cards for lossless lines (`type=0`) are now accepted in the executable network subset using the same uniform-line stamp semantics as `NSEG=1`; the previous deferred "TL with NSEG=... not yet supported" runtime warning path is removed.
- Phase 2 traceability coverage is now stricter: the enforced PH2-CHK-007 matrix explicitly maps newer EX current-source, LD load-family, TL subset, and PT/NT deferred-portability corpus classes, and CI now requires those row IDs to remain present.
- PT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; PT electrical semantics are still pending and currently ignored.
- NT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; NT electrical semantics are still pending and currently ignored.
- CLI report contract v1 now includes stable operator tables for source/load definitions: `SOURCES` (`TYPE TAG SEG I4 V_RE V_IM`) and `LOADS` (`TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3`) sections, emitted in deterministic order between `FEEDPOINTS` and `CURRENTS`.
- Scriptability contracts now explicitly lock stdout ordering around the new tables (`FEEDPOINTS -> SOURCES -> CURRENTS`) and enforce that `LOADS` table output stays report-only on stdout while warnings remain stderr-only.
- Loaded-case tracking now also locks the default Hallen hard-fail contract on `dipole-loaded` (non-collinear topology error, exit code 1, and no report on stdout) to keep Phase 1 gap behavior explicit and deterministic.

### Added

- RP card execution is now wired into the CLI report path.
- Text reports now include a `RADIATION_PATTERN` section when one or more `RP` cards are present.
- Added corpus regression deck `corpus/dipole-freesp-rp-51seg.nec` and contract coverage for pattern-table rendering.
- Added `docs/benchmarks.md` with a validated three-host baseline comparison (local workstation, T480, Raspberry Pi 5).
- Added a collaboration efficiency guide with rate-limit-aware prompting patterns at `docs/copilot-efficiency-guide.md`.
- Added `docs/par011-dropin-evidence-memo.md` as a dedicated evidence scaffold for deferred 4nec2 drop-in compatibility work.
- **GPU kernel stubs** (Phase A expansion): Extended `nec_accel::gpu_kernels` module with additional kernel scaffolds:
  - `HallenRhsGpuKernel` for Hallén RHS vector computation with excitation handling
  - `PocklingtonMatrixGpuKernel` for matrix assembly with segment-pair element distribution
  - `KernelTiming` struct for capturing prep/exec/retrieval timing data (microsecond resolution)
  - 4 new unit tests for kernel construction and sizing (12 total nec_accel lib tests)
  - GPU-compatible data structures prepared for future CUDA/OpenCL replacement
- **CLI GPU FR integration** (Phase B): Added `--gpu-fr` command-line flag to dispatch radiation pattern computation to GPU kernel stub:
  - Far-field points routed through `HallenFrGpuKernel` when flag is enabled
  - Maintains full output parity with CPU far-field path
  - Integration tested with 6 GPU stub tests + existing exec_modes contract tests
- **Performance benchmarking** (Phase D): Added optional timing instrumentation for GPU kernel operations:
  - `--bench` CLI flag to enable benchmarking mode
  - `--bench-format <human|csv|json>` to emit machine-readable benchmark records while preserving the standard human-readable report output
  - `FNEC_GPU_BENCH` environment variable control (set to "1" to enable timing collection)
  - `compute_hallen_fr_point_with_timing()` API returns `(result, KernelTiming)` tuples
  - Timing breakdown: prep (coordinate transform), exec (far-field summation), retrieval (stub: zero)
  - Ready for future GPU timing collection once real CUDA/OpenCL kernels are wired
- Corpus validation framework already supports pattern and current-gate scenarios (Phase C); enhancements documented for future use.

### Changed

- Added missing `GE` cards to three corpus decks (`dipole-ld-series-rc-51seg`, `dipole-ld-series-rl-51seg`, `tl-two-dipoles-linked-seg0`) so `corpus_deck_sanity` passes consistently in local hooks and CI.
- Native CLI startup now auto-selects execution mode when `--exec` is omitted by running a quick execution probe (CPU threads, frequency-point count, and accelerator dispatch availability) and choosing among `cpu`/`hybrid`/`gpu` heuristically for the current workload shape.
- Consolidated benchmark documentation into a single canonical file (`docs/benchmarks.md`) and removed the duplicate `docs/benchmark.md` shim.
- Benchmark docs now explicitly map reported numbers to four execution modes: CPU single-thread, CPU multithread, GPU, and hybrid (CPU multithread + GPU), with a dedicated local four-mode coverage result block.
- Sinusoidal topology gating advanced through A4: the solver now accepts collinear wire-chain geometries (including multi-wire chains) with orientation/order-agnostic endpoint connectivity checks, and still falls back for disconnected/branched/unsupported topologies.
- Added a gitignored benchmark host env pattern (`.benchmark-hosts.env` with tracked `.benchmark-hosts.env.example`) and updated `scripts/pi-remote-benchmark.sh` to accept env defaults (`FNEC_BENCH_TARGET`, `FNEC_REMOTE_REPO_SUBDIR`).
- Remote benchmark tooling now supports execution-mode sweeps (`FNEC_BENCH_EXECS`) and records `diag_spread` plus `sin_rel_res` in benchmark CSV output and comparison reports.
- Added `scripts/pi-benchmark-summary.sh` to summarize a single benchmark CSV without pandas or ad hoc shell commands.
- Added `sin_rel_res` to CLI diagnostics: the sinusoidal basis relative residual captured before any fallback decision, enabling solver-quality trending across runs (0.0 for non-sinusoidal modes).
- Added `diag_spread` to CLI diagnostics as a conditioning proxy (ratio of max/min diagonal magnitudes of the solved system matrix), enabling quick stability checks in automation.
- Added sinusoidal A2 regression checks that compare sinusoidal-mode impedance output against Hallen on `dipole-freesp-51seg` and `frequency-sweep-dipole` corpus decks.
- Sinusoidal solver routing is now topology-gated for A1: it runs only on single-wire collinear decks and otherwise falls back explicitly to pulse with `sinusoidal->pulse(topology)` diagnostics.
- Completed PAR-008 coverage-matrix scope: NEC-5 validation scenario classes are now explicitly mapped to current corpus-backed in-scope equivalents, with out-of-scope classes and rationale documented for phased deferral.
- Updated support and CLI docs to mark RP pattern output as implemented in the text-report path (with remaining export/near-field scope still deferred).
- Corpus validation now numerically checks stored RP pattern samples instead of only asserting pattern-table presence.
- Corpus validation now also checks the stored vertical/horizontal gain columns and axial ratio for locked RP sample angles.
- RP corpus angle coverage was expanded from 2 locked sample angles to 7 locked angles across the theta sweep.
- Added a second RP corpus case with non-z-axis geometry and multi-phi sample locking to validate true azimuth-cut coverage.
- Corpus validation now also records external-reference deltas for RP pattern samples when `external_reference_candidate.pattern_samples` is present.
- Added `nec2c` external RP sample candidates for the multi-phi x-axis corpus case so parity tracking now covers both current RP decks.
- RP corpus cases can now opt into external-pattern CI gates via `ExternalGain_absolute_dB` and `ExternalAxialRatio_absolute` in `tolerance_gates`.
- Corpus validation now also supports optional external impedance CI gates (`ExternalR_*`/`ExternalX_*`) for scalar, multi-source, and frequency-sweep candidates.
- Enabled the first external impedance CI-gated case (`frequency-sweep-dipole`) with absolute candidate thresholds (`ExternalR_absolute_ohm=15.0`, `ExternalX_absolute_ohm=50.0`).
- Enabled a second external impedance CI-gated case (`dipole-ground-51seg`) with absolute candidate thresholds (`ExternalR_absolute_ohm=10.0`, `ExternalX_absolute_ohm=30.0`).
- Roadmap now defines a required benchmark-mode matrix across all target classes: CPU single-threaded, CPU multithreaded, and GPU offload.
- CLI now accepts `--exec <cpu|hybrid|gpu>` for real runs; `hybrid`/`gpu` are scaffolded execution modes that currently fall back to CPU with explicit diagnostics.
- `--exec hybrid` now performs coarse-grain multithreaded FR sweep solving (parallel per-frequency solve with ordered report output); GPU execution remains scaffolded.
- `--exec hybrid` now uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered report output; GPU-candidate lane points currently emit explicit fallback warnings and execute on CPU until GPU kernels are wired.
- Hybrid and GPU-mode fallback routing now flows through a concrete `nec_accel` dispatch API (`dispatch_frequency_point`) so future GPU kernel wiring has a stable integration seam.
- Added an opt-in accelerator stub dispatch path (`FNEC_ACCEL_STUB_GPU=1`) so `DispatchDecision::RunOnGpu` can be exercised end-to-end in CLI hybrid and gpu execution flows without changing output contracts.
- Added a tracked parity item for filename-steered 4nec2 solver-binary drop-in compatibility mode, including contract-preservation and throughput validation goals.
- Retargeted 4nec2 external-kernel drop-in compatibility work to a farther-future window (Phase 4-5) after assessing real NEC2MP replacement artifacts and integration scope.
- Expanded PAR-011 with an implementation discovery checklist (binary-name matrix, install/invocation contract, file side effects, dependency surface, fixtures, and benchmark protocol) to reduce future re-research cost.
- Added GNU NEC (`https://sourceforge.net/projects/gnu-nec/`) as an additional open-source reference candidate in architecture and PAR-011 source notes.
- Refined filename-steered 4nec2 compatibility warnings to explicitly report whether execution was auto-steered or an explicit `--exec` value was preserved.
- Extended drop-in compatibility contract tests to cover both `nec2dxs*` and `4nec2*` alias-name detection paths.
- Populated `docs/par011-dropin-evidence-memo.md` with concrete NEC2MP artifact evidence (inventory, readme findings, SHA256 fingerprints) and a phased docs-only PAR-011 implementation plan with `AT-PAR011-*` acceptance tests.
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- **PH2-CHK-003 — LD/TL/NT implemented semantics (2026-05-10)**: LD cards (types 0–5) and TL lossless-line cards (`type=0`) are now parsed in `nec_parser` and applied as impedance stamps in the solver; NT cards are parsed for staged portability and emit a deferred-support warning instead of an unknown-card warning. 5 `ld_loads.rs` and 3 `tl_cards.rs` integration tests updated to Phase-2 assertions; 14 corpus reference entries in `reference-results.json` updated (3 LD loaded-value cases, 4 TL coupled-dipole cases, 7 NT deferred-warning cases); `parser_warnings.rs`, `report_contract.rs`, and `scriptability_contract.rs` tests updated to Phase-2 contracts.
- **PH2-CHK-007 — NEC-5 validation matrix ticked done (2026-04-30)**: The PH2-CHK-007 traceability matrix in `docs/corpus-validation-strategy.md` (row IDs `PH2N5-001` … `PH2N5-010`) carries explicit `in-scope implemented` / `in-scope deferred` / `out-of-scope` statuses with corpus case mappings, and `phase2_nec5_matrix_rows_are_traceable_to_corpus_cases` in `apps/nec-cli/tests/corpus_validation.rs` enforces row-ID presence, status validity, and corpus-case existence in CI. The PH2-CHK-007 done signal is therefore already met by prior PH2-CHK-005 work; this entry records the roadmap tick.
- **PH2-CHK-002 — Buried/near-ground guardrails ticked done (2026-04-30)**: `buried_wire_geometry_error` in `apps/nec-cli/src/geometry_validation.rs` fails fast with an actionable diagnostic when active-ground decks include `z<0` segments; `buried_wire_with_active_ground_fails_fast_with_actionable_error` and `near_ground_wire_with_active_ground_runs_without_deferred_warning` regression tests lock both branches in `apps/nec-cli/tests/ground_diagnostics.rs`; supported `dipole-gn2-near-ground-51seg` and unsupported `dipole-gn2-buried-unsupported` corpus fixtures are gated by warning / forbidden-warning / `expected_hallen_error_contains` contracts; `par002_ground_checklist_cases_are_present_and_contracted` enforces the matrix. The PH2-CHK-002 done signal is therefore already met by prior PH2-CHK-001 work; this entry records the roadmap tick.
- **PH2-CHK-004 — Report/table parity ticked done (2026-04-30)**: All 6 table sections implemented and CI-locked — `FEEDPOINTS`, `SOURCES`, `LOADS`, `CURRENTS`, `RADIATION_PATTERN`, `SWEEP_POINTS`; 5 report-contract tests in `apps/nec-cli/tests/report_contract.rs` lock headers, row parsing, section presence, and per-frequency block ordering (`FEEDPOINTS → SOURCES → LOADS → CURRENTS → SWEEP_POINTS`); 7 scriptability-contract tests in `apps/nec-cli/tests/scriptability_contract.rs` enforce machine-parseable stdout and stderr-only warnings. The PH2-CHK-004 done signal is already met by prior PH2-CHK-003 + 0.3.0 report work; this entry records the roadmap tick.
- **PH2-CHK-008 — Scriptability preservation ticked done (2026-04-30)**: 7 scriptability-contract tests lock stdout-only report stream, stderr-only warnings/bench records, `LOADS`-on-stdout (Phase-2), and exit-code contracts (code 1 on file-read error, code 2 on bad args); 11 core-flags-contract tests lock `--solver`, `--pulse-rhs`, `--exec`, `--bench-format` error/usage contracts and combined-flag success run. All 18 tests pass with zero regression after Phase-2 table and diagnostic additions; this entry records the roadmap tick.

## 0.2.0 — 2026-05-01

### Added

- **GM/GR card support**: GM (Geometry Move) and GR (Geometry Repeat) cards are now parsed and
  applied during geometry expansion. GM rotates/translates wire ranges (in-place or as copies with
  incremented tags); GR repeats all existing wires by successive z-axis rotations.
- **Segment current distribution table**: CLI output now includes a `CURRENTS` section listing
  TAG, SEG, I_RE, I_IM, I_MAG, I_PHASE (deg) for every segment after the feedpoint table.
- **Multi-wire Hallen fix**: per-wire homogeneous constants and endpoint constraints; passive wires
  now correctly receive zero RHS. Yagi and multi-source corpus validation now produces correct
  impedances (Yagi: 30.6+j5.0 Ω, multi-source: 152.4+j31.6 Ω each port).

### Changed

- GE I1=-1 warning now says "requests below-ground wire handling (no image method);
  treating as free-space" instead of a generic "not yet supported" message.
- GE I1=other unknown values now include the valid range hint
  `(valid values: 0=free-space, 1=PEC image, -1=below-ground)`.
- Updated corpus reference values for yagi-5elm-51seg and multi-source decks.

## 2026-04-24

### Added

- Added Phase 1 `GN` card support for perfect-ground (`GN 1`) Hallen runs.
- Added PEC image-method contribution path in Hallen matrix assembly.
- Added parser and solver tests that cover GN parsing and ground-aware matrix behavior.

### Changed

- Updated corpus ground regression reference (`dipole-ground-51seg`) to GN-aware Hallen values.
- Updated support boundary documentation to reflect current GN status (`GN 1` supported; Sommerfeld/Norton deferred).

## 2026-04-22

### Added

- Standard frontmatter requirements for all docs under `docs/`.
- Requirements, steering, roadmap, architecture, design, backlog, SBOM, and memory structure.
- CI automation design for docs stamping and validation.

### Changed

- Documented recent MoM kernel investigations and convergence behavior in new solver notes.
- Added an applied-math reference document with key EFIE/Pocklington/Hallen formulas.
- Added an implementation plan for continuity-enforcing rooftop/sinusoidal basis work.
- Added prominent README support/sponsoring note.
- Added project-local temporary work folder ignore guidance.
- Added regression tests for Hallén RHS symmetry/shape and Hallén/continuity solver behavior.
- Added CLI solver mode selection (`--solver hallen|pulse|continuity`) and single-chain continuity routing.
- Added documented mode benchmark deltas across segment counts in solver findings.
- Added explicit Hallen vs Pocklington matrix routing by solver mode and post-change benchmark notes.
- Added NEC2 reference-inspired pulse RHS wavelength normalization path:
  $$\\frac{1}{dl\\,\\lambda}$$
  and validation notes.

<!-- Compare links.
     Only the contiguous tagged range is linkable: v0.4.0, v0.5.0, v0.6.0,
     v0.8.0 and v0.9.0 were released without tags, so there is no ref to compare
     against and inventing one would be worse than the gap (FND-043). -->

[Unreleased]: https://github.com/dc0sk/fnec-rust/compare/v0.15.0...HEAD
[0.15.0]: https://github.com/dc0sk/fnec-rust/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/dc0sk/fnec-rust/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/dc0sk/fnec-rust/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/dc0sk/fnec-rust/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/dc0sk/fnec-rust/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/dc0sk/fnec-rust/releases/tag/v0.10.0
[0.7.0]: https://github.com/dc0sk/fnec-rust/releases/tag/v0.7.0
