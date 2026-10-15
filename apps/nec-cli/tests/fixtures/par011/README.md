---
project: fnec-rust
doc: apps/nec-cli/tests/fixtures/par011/README.md
status: living
last_updated: 2026-10-15
---

# PAR-011 kernel fixtures

These files are the input and output of 4nec2's external kernel, captured from a real 4nec2 run. Each `NAME.inp` is the file **4nec2 itself wrote** and handed to the kernel. Each `NAME.out` is what the kernel wrote back, which 4nec2 then parsed.

Both are kept byte-exact, with CRLF line ends and tabs. The `.gitattributes` here turns off end-of-line conversion. The drop-in kernel's layout gate (PAR-011 P2a) compares fnec's output with these files.

## Provenance

- **Host:** 4nec2 V5.9.3, `4nec2.exe` sha256 `a009931034672a6fce95d8bedf0a0d3b6eb93e1ca748f1f2625a06ea37e9507b`. It ran under Wine 10.0 (Ubuntu 10.0~repack-12ubuntu1) in a win64 prefix on Linux, displayed on Xvfb, and was driven with xdotool.
- **Kernel:** `nec2dxs1K5.exe`, the build bundled with 4nec2 ("Merged nec2d/som2d file created by Arie Voors", build 2.7, 30-jan-08). Its sha256 is `70cf205821423623bded9a2a3ba776ca3adae3eb5d39d4ef7b67637483405841`. This is not the NEC2MP package fingerprinted in `docs/par011-dropin-evidence-memo.md`.
- **Captured:** 2026-10-15.

## How each pair was made

The deck `models\zzpar011\NAME.nec` was opened in 4nec2, then run with *Calculate → NEC output-data → Use original file → Generate*. 4nec2 wrote the `.inp` into `..\out\` and ran the kernel. The pair was then copied out of `..\out\`.

- **Pattern grids.** The decks request coarse 30° grids, so the fixtures stay small.
- **`ex3_gn2_sy`.** This is 4nec2's shipped `models\Example3.nec`, run with *Far Field pattern* instead of the original file. Its `.inp` therefore shows 4nec2's own preprocessing: `SY` symbols evaluated, a `#12` wire gauge turned into a radius, a `50%` segment reference resolved, comments stripped, and 4nec2's `RP 0 19 73 1003 -90 0 5 5` substituted.
- **`dip_sweep.inp`.** 4nec2 deletes this file once the run finishes. It was copied out of `..\out\` while the kernel was still running.

| Pair | What it exercises |
|---|---|
| `dip_free` | half-wave dipole, free space, `RP` |
| `dip_pec` | horizontal dipole over perfect ground (`GN 1`) |
| `dip_gn0` | the same dipole over finite ground, reflection-coefficient approximation (`GN 0`) |
| `dip_ld` | series RLC load plus wire conductivity (`LD 0`, `LD 5`) |
| `ld_range` | a load on a segment range (`LD 0 1 3 5`) |
| `dip_sweep` | a five-step `FR` sweep; its `.inp` keeps `EN` |
| `two_src` | two voltage sources on two wires |
| `tl_pair` | a transmission line between two dipoles (`TL`) |
| `nt_net` | a two-port network (`NT`) |
| `gm_gx` | `GM` move-and-copy, then `GX` reflection |
| `sp_patch` | a wire and a surface patch (`SP`) |
| `two_fr` | two `FR` groups, each followed by `XQ` |
| `nf` | near electric field (`NE`) |
| `err_seg` | a source on a segment that does not exist: the kernel's failure output, which has no `RUN TIME` line |
| `ex3_gn2_sy` | 4nec2's `Example3`: `SY`, gauge, `%`, Sommerfeld ground (`GN 2`) computed inside the kernel |
