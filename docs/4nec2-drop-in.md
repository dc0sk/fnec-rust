---
project: fnec-rust
doc: docs/4nec2-drop-in.md
status: living
last_updated: 2026-10-16
---

# fnec as 4nec2's kernel

4nec2 is a Windows front end for NEC-2. It does its solving by running an external kernel, `nec2dxs*.exe`, and reading the report that kernel writes. fnec can stand in for that kernel: 4nec2 stays as it is, and fnec solves (PAR-011).

## What has been tested, and what has not

This is a drop-in at an early evidence tier. Read this list before installing.

- **Tested:** 4nec2 V5.9.3 under **Wine 10.0** (win64 prefix) on Linux. Every deck type in `apps/nec-cli/tests/fixtures/par011/` that fnec prints was run through 4nec2's own *Generate* (`scripts/par011-wine-e2e.sh`), and 4nec2 read fnec's report each time:
  - a free-space dipole;
  - loads;
  - a perfect and a finite ground;
  - a frequency sweep;
  - two sources;
  - `GM`/`GX` symmetry;
  - a near field.
- **Not tested:** real Windows, and therefore Windows Defender and SmartScreen. The binary is **unsigned**.
- **64-bit Windows only.** 4nec2 itself is 32-bit, and starting a 64-bit kernel from it works under Wine's win64 prefix. There is no 32-bit build.
- **CPU only:** the kernel runs fnec with `--exec cpu`.
- **fnec's answer, not NEC-2's.** The report has NEC-2's layout, but fnec solves it, with a Hallén formulation that differs from NEC-2's own. Its numbers differ from the old kernel's by fnec's discretisation; both converge to the same values as the mesh is refined. On a coarse mesh fnec's reactance error is 2.5–3× nec2c's (a dipole at 11 segments a half-wave: 4.8 % of |Z|, at 81: 1.2 %), so a deck meshed for NEC-2's accuracy needs 3–4× the segments for the same reactance in fnec (FND-315). A line before `RUN TIME` names fnec as the solver.
- **What fnec cannot print yet is refused.** For those decks, 4nec2 shows "NEC error?", and F8 shows fnec's reason:
  - networks and transmission lines (`NT`, `TL`);
  - Sommerfeld ground (`GN 2`);
  - arcs, helices and tapers (`GA`, `GH`, `GC`);
  - excitations other than a voltage source;
  - more than one `FR` card;
  - several `RP`/`NE`/`XQ` runs;
  - see `docs/cli-guide.md` § *NEC-2D output* for the rest.

## Build

```bash
scripts/build-windows-kernel.sh            # writes ~/.cache/fnec-windows-kernel/
```

The script cross-compiles fnec for `x86_64-pc-windows-gnu` in a rust container pinned by digest. It checks that the binary imports only DLLs every Windows 10+ install ships, and writes `fnec.exe` plus six copies under 4nec2's kernel names. `BUILD-INFO` records the image, the compiler, the linker and every file's sha256. The build is not yet byte-reproducible: two builds of one commit have differed.

## Install

4nec2 **chooses its kernel by segment count** (observed: decks of 9 to 84 segments ran `Nec2dXS1k5`, and a 2001-segment deck ran `Nec2dXS3k0`). So replace **all six** names. Replacing one would leave larger decks on the old kernel, silently.

1. Close 4nec2.
2. In 4nec2's `exe` folder, make a backup folder `backup-YYYYMMDD-HHMMSS` and copy the six `nec2dxs*.exe` into it. Record their sha256 there, for example in `MANIFEST.sha256`.
3. Copy the six files from the build output over the originals: `nec2dxs500.exe`, `nec2dxs1K5.exe`, `nec2dxs3k0.exe`, `nec2dxs5k0.exe`, `nec2dxs8k0.exe`, `nec2dxs11k.exe`.
4. Start 4nec2 and run a deck. The report's last lines name fnec (`SOLVED BY FNEC …`). The report is in `out\NAME.out`; F8 opens it in 4nec2.

## Roll back

1. Close 4nec2.
2. Copy the six originals back from the backup folder.
3. Check them against the recorded sha256 (`sha256sum -c MANIFEST.sha256` under Wine/Linux; `certutil -hashfile` on Windows).

`scripts/par011-wine-e2e.sh` does all of this itself: back up, install, run, check the reports, restore, verify.

## How it works

4nec2 writes the deck to `out\NAME.inp`, and the two paths to `exe\nec2d.tmp`. It then runs `4nec2.bat`, which runs the kernel with no arguments and `nec2d.tmp` on stdin. Started under one of those names with no arguments, fnec is the kernel (`fnec nec2d-kernel`):

1. It reads the two paths.
2. It solves the deck with `--output-format nec2d`.
3. It writes the report to the output file, ending in `RUN TIME`, which is how 4nec2 knows the run succeeded.

A deck fnec refuses or cannot read gets its reason written there instead, with no `RUN TIME`; 4nec2 then shows it as a NEC error. The full observed contract is in `docs/par011-dropin-evidence-memo.md`.

## Licence

fnec is GPL-3.0-only. A copy you pass on carries the same licence and must come with its source or a written offer of it; the source is this repository at the commit `BUILD-INFO` names.
