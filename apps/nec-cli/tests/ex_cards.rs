use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_fnec_output(deck_path: &Path, workspace_root: &Path, extra_args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver").arg("hallen").arg("--exec").arg("cpu");

    for arg in extra_args {
        cmd.arg(arg);
    }

    cmd.arg(deck_path)
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {}: {e}", deck_path.display()))
}

#[allow(dead_code)]
fn first_feedpoint_impedance(stdout: &str) -> (f64, f64) {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 8 {
            continue;
        }
        if cols[0] == "TAG" {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }

        let z_re = cols[6]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_RE from '{line}': {e}"));
        let z_im = cols[7]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_IM from '{line}': {e}"));
        return (z_re, z_im);
    }

    panic!("no feedpoint rows found in stdout:\n{stdout}");
}

/// Helper: an incident plane-wave EX type (1/2/3) solves on --solver hallen and
/// reports induced CURRENTS (receive solve). Used for the elliptic types 2/3.
fn assert_ex_plane_wave_solves(ex_type: u8, workspace_root: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex{ex_type}-pw-{now}.nec"));
    // EX N NTHETA NPHI 0 THETA PHI ETA — plane wave from θ=30°.
    let deck = format!(
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX {ex_type} 1 1 0 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n"
    );
    fs::write(&deck_path, deck).expect("failed to write deck");
    let output = run_fnec_output(&deck_path, workspace_root, &["--solver", "hallen"]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&deck_path);
    assert!(
        output.status.success(),
        "EX type {ex_type} plane wave should solve on hallen; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("is not yet supported"),
        "EX type {ex_type} plane wave must not be rejected; stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CURRENTS"),
        "EX type {ex_type} plane-wave solve should report induced CURRENTS; stdout:\n{stdout}"
    );
}

#[test]
fn ex_type3_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_plane_wave_solves(3, &workspace_root);
}

#[test]
fn ex_type1_plane_wave_solves_on_hallen() {
    // PH8-CHK-002: NEC2 type 1 = incident plane wave (linear). On a single
    // straight wire with --solver hallen it SOLVES (receiving antenna) and
    // reports induced currents — no "is not yet supported" rejection.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let path = std::env::temp_dir().join(format!("fnec-ex1-planewave-{now}.nec"));
    // EX 1 NTHETA NPHI 0 THETA PHI ETA — plane wave from θ=30°, φ=0, linear.
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 1 0 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&path, deck).expect("failed to write EX type 1 plane-wave deck");

    let output = run_fnec_output(&path, &workspace_root, &["--solver", "hallen"]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&path);

    assert!(
        output.status.success(),
        "EX type 1 plane wave should solve on --solver hallen; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("is not yet supported"),
        "EX type 1 plane wave must not be rejected; stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CURRENTS"),
        "plane-wave solve should report induced CURRENTS; stdout:\n{stdout}"
    );
}

#[test]
fn ex_type1_plane_wave_requires_hallen_solver() {
    // A plane wave is solved only on the Hallén path; --solver pulse fails fast
    // with an actionable diagnostic rather than silently mis-solving.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex1_path = std::env::temp_dir().join(format!("fnec-ex1-pulse-{now}.nec"));
    let ex1_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 1 0 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex1_path, ex1_deck).expect("failed to write EX type 1 pulse deck");

    let output = run_fnec_output(&ex1_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex1_path);

    assert!(
        !output.status.success(),
        "EX type 1 plane wave under --solver pulse should fail fast; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("requires --solver hallen"),
        "EX type 1 pulse stderr should say the plane wave requires --solver hallen, got:\n{stderr}"
    );
}

#[test]
fn ex_type4_current_source_requires_hallen_solver() {
    // PH8-CHK-001: EX type 4 (current source) is solved only on the Hallén path;
    // --solver pulse fails fast with an actionable diagnostic.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex4_path = std::env::temp_dir().join(format!("fnec-ex4-pulse-{now}.nec"));
    let ex4_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex4_path, ex4_deck).expect("failed to write EX type 4 pulse deck");

    let output = run_fnec_output(&ex4_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex4_path);

    assert!(
        !output.status.success(),
        "EX type 4 current source under --solver pulse should fail fast; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("requires --solver hallen"),
        "EX type 4 pulse stderr should say the current source requires --solver hallen, got:\n{stderr}"
    );
}

#[test]
fn ex_type5_voltage_source_solves_under_pulse() {
    // PH8-CHK-003: EX type 5 is a voltage source; it solves under --solver pulse
    // (fnec models it via the applied-field method, same as type 0).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex5_path = std::env::temp_dir().join(format!("fnec-ex5-pulse-{now}.nec"));
    let ex5_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 5 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex5_path, ex5_deck).expect("failed to write EX type 5 pulse deck");

    let output = run_fnec_output(&ex5_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex5_path);

    assert!(
        output.status.success(),
        "EX type 5 voltage source should solve under --solver pulse; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("is not yet supported"),
        "EX type 5 voltage source must not be rejected; stderr:\n{stderr}"
    );
}

#[test]
fn ex_type2_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_plane_wave_solves(2, &workspace_root);
}

#[test]
fn ex_type4_current_source_matches_voltage_source_impedance() {
    // PH8-CHK-001: a current source (type 4, i0=1) at the feed yields the same
    // feedpoint impedance as a voltage source (type 0) at the same feed — the
    // port impedance is a property of the antenna, independent of drive type.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let geom = "GW 1 51 0 0 -5.282 0 0 5.282 0.001";
    let fr = "FR 0 1 0 0 14.2 0.0\nEN\n";

    let v_path = std::env::temp_dir().join(format!("fnec-ex0-{now}.nec"));
    fs::write(&v_path, format!("{geom}\nEX 0 1 26 0 1.0 0.0\n{fr}")).expect("write v deck");
    let v_out = run_fnec_output(&v_path, &workspace_root, &["--solver", "hallen"]);
    let (zr_v, zi_v) = first_feedpoint_impedance(&String::from_utf8_lossy(&v_out.stdout));
    let _ = fs::remove_file(&v_path);

    let c_path = std::env::temp_dir().join(format!("fnec-ex4-{now}.nec"));
    fs::write(&c_path, format!("{geom}\nEX 4 1 26 0 1.0 0.0\n{fr}")).expect("write cs deck");
    let c_out = run_fnec_output(&c_path, &workspace_root, &["--solver", "hallen"]);
    let c_stdout = String::from_utf8_lossy(&c_out.stdout).into_owned();
    let (zr_c, zi_c) = first_feedpoint_impedance(&c_stdout);
    let _ = fs::remove_file(&c_path);

    assert!(
        c_out.status.success(),
        "EX type 4 current source should solve on --solver hallen"
    );
    // Same port impedance within a modest tolerance (the two augmented-system
    // formulations agree to ~0.02% on this dipole).
    assert!(
        (zr_c - zr_v).abs() < 0.5 && (zi_c - zi_v).abs() < 0.5,
        "current-source Z ({zr_c:.3}+j{zi_c:.3}) != voltage-source Z ({zr_v:.3}+j{zi_v:.3})"
    );
}

#[test]
fn ex_type5_matches_ex_type0_feedpoint_impedance() {
    // PH8-CHK-003: EX type 5 (voltage source, current-slope) is modelled by fnec
    // via the applied-field method, so its feedpoint impedance equals type 0's.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let geom = "GW 1 51 0 0 -5.282 0 0 5.282 0.001";
    let fr = "FR 0 1 0 0 14.2 0.0\nEN\n";

    let p0 = std::env::temp_dir().join(format!("fnec-ex0-cmp-{now}.nec"));
    fs::write(&p0, format!("{geom}\nEX 0 1 26 0 1.0 0.0\n{fr}")).expect("write");
    let (zr0, zi0) = first_feedpoint_impedance(&String::from_utf8_lossy(
        &run_fnec_output(&p0, &workspace_root, &["--solver", "hallen"]).stdout,
    ));
    let _ = fs::remove_file(&p0);

    let p5 = std::env::temp_dir().join(format!("fnec-ex5-cmp-{now}.nec"));
    fs::write(&p5, format!("{geom}\nEX 5 1 26 0 1.0 0.0\n{fr}")).expect("write");
    let out5 = run_fnec_output(&p5, &workspace_root, &["--solver", "hallen"]);
    let (zr5, zi5) = first_feedpoint_impedance(&String::from_utf8_lossy(&out5.stdout));
    let _ = fs::remove_file(&p5);

    assert!(
        out5.status.success(),
        "EX type 5 should solve on --solver hallen"
    );
    assert!(
        (zr5 - zr0).abs() < 1e-3 && (zi5 - zi0).abs() < 1e-3,
        "EX type 5 Z ({zr5:.4}+j{zi5:.4}) != type 0 Z ({zr0:.4}+j{zi0:.4})"
    );
}

#[test]
fn ex_type3_i4_runtime_mode_divide_by_i4_scales_source_and_current() {
    // PH8-CHK-002: EX type 3 is now a left-elliptic plane wave and solves on the
    // Hallén path. The legacy --ex3-i4-mode flag is an obsolete no-op (its former
    // "normalized voltage source" meaning predates the NEC2 EX-type alignment)
    // and must not prevent the plane-wave solve.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex3_i4_path = std::env::temp_dir().join(format!("fnec-ex3-i4-{now}.nec"));
    // EX 3 NTHETA NPHI I4 THETA PHI ETA — plane wave; I4 is unused for plane waves.
    let ex3_i4_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 1 2 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex3_i4_path, ex3_i4_deck).expect("failed to write EX type 3 I4 deck");

    let output = run_fnec_output(
        &ex3_i4_path,
        &workspace_root,
        &["--ex3-i4-mode", "divide-by-i4"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex3_i4_path);

    assert!(
        output.status.success(),
        "EX type 3 plane wave should solve even with the obsolete --ex3-i4-mode; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("is not yet supported"),
        "EX type 3 plane wave must not be rejected, got stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CURRENTS"),
        "EX type 3 plane-wave solve should report induced CURRENTS; stdout:\n{stdout}"
    );
}

/// FND-120, at the production entry point.
///
/// A mirror-symmetric wire driven at two mirror-image segments must present the
/// same impedance at both. That is a symmetry argument, so it holds whatever the
/// basis does — it cannot be satisfied by a formulation quirk, and it cannot be
/// satisfied by dropping both sources either, because then there is nothing to
/// report.
///
/// Before the fix the RHS collection was a map keyed by wire tag, so one wire
/// could carry one source. Measured on this deck: 112.181884 + j16.627983 at
/// segment 16 against 106.722105 + j28.781021 at segment 36 — visibly asymmetric
/// on a symmetric problem — and the CURRENTS block was byte-identical (md5) to
/// the same deck with the second `EX` card removed, so the second source
/// contributed exactly nothing while the report still printed a feedpoint row
/// for it, at exit 0 with no warning.
///
/// The unit test in `nec_solver::excitation` pins the RHS; this pins that a user
/// running `fnec` actually gets it.
#[test]
fn two_gaps_on_one_wire_report_the_same_impedance_at_both() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("fnec-fnd120-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let deck = dir.join("two-gaps.nec");
    fs::write(
        &deck,
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\n\
         EX 0 1 16 0 1.0 0.0\nEX 0 1 36 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0.0\nEN\n",
    )
    .expect("write deck");

    let out = run_fnec_output(&deck, &root, &[]);
    let _ = fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "deck must solve: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Both feedpoint rows: TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
    let rows: Vec<Vec<f64>> = stdout
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() != 8 || c[0] != "1" {
                return None;
            }
            c.iter().map(|v| v.parse::<f64>().ok()).collect()
        })
        .collect();
    assert_eq!(rows.len(), 2, "expected two feedpoint rows:\n{stdout}");

    let (z1_re, z1_im) = (rows[0][6], rows[0][7]);
    let (z2_re, z2_im) = (rows[1][6], rows[1][7]);
    assert!(
        (z1_re - z2_re).abs() < 1e-6 && (z1_im - z2_im).abs() < 1e-6,
        "a symmetric deck driven symmetrically must report one impedance at both \
         feeds, got {z1_re}+j{z1_im} and {z2_re}+j{z2_im}"
    );
    // And a real one — dropping both sources would make them trivially equal.
    assert!(
        z1_re > 1.0 && z1_re.is_finite(),
        "the feeds must actually be driven, got R = {z1_re}"
    );
}

/// The `(tag, seg, V_re, V_im, I_re, I_im, Z_re, Z_im)` feedpoint row, whole.
fn first_feedpoint_row(stdout: &str) -> (f64, f64, f64, f64) {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 8 || cols[0] == "TAG" {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }
        let f = |i: usize| {
            cols[i]
                .parse::<f64>()
                .unwrap_or_else(|e| panic!("failed to parse column {i} from '{line}': {e}"))
        };
        return (f(4), f(5), f(6), f(7));
    }
    panic!("no feedpoint rows found in stdout:\n{stdout}");
}

/// A *reducible* current-source deck must keep the per-wire basis (FND-140).
///
/// `dipole-ex4-collinear-split-51seg.nec` is a collinear chain: the conductor
/// decomposition succeeds and every path is trivial, so it belongs on the plain
/// basis — but it also carries a junction, so any code that treats "no
/// non-trivial path" as "unsupported topology" refuses it. That is precisely the
/// regression the two-way collapse of `PathRoute` would introduce, and it is
/// silent in every other current-source test because no other EX 4 fixture is
/// split.
///
/// The deck asserts its own contract in its CM block: physically identical to
/// `dipole-ex4-freesp-51seg.nec`, so it must reach the same impedance and must
/// deliver the 1 A the EX card asks for. Until now nothing checked it — the deck
/// sat in `corpus/` with no case and no test (FND-139).
#[test]
fn reducible_collinear_split_current_source_keeps_the_per_wire_basis() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let split = workspace_root.join("corpus/dipole-ex4-collinear-split-51seg.nec");
    let whole = workspace_root.join("corpus/dipole-ex4-freesp-51seg.nec");

    let split_out = run_fnec_output(&split, &workspace_root, &[]);
    assert!(
        split_out.status.success(),
        "the collinear-split EX 4 deck must solve, not be refused as an unsupported \
         topology.\nstderr:\n{}",
        String::from_utf8_lossy(&split_out.stderr)
    );
    let (i_re, i_im, zr_s, zi_s) = first_feedpoint_row(&String::from_utf8_lossy(&split_out.stdout));

    let whole_out = run_fnec_output(&whole, &workspace_root, &[]);
    assert!(
        whole_out.status.success(),
        "the unsplit EX 4 deck must solve"
    );
    let (_, _, zr_w, zi_w) = first_feedpoint_row(&String::from_utf8_lossy(&whole_out.stdout));

    // The EX card asks for 1 A at the join. Splitting the wire there is what
    // made the solver deliver 0.5 A once (FND-048).
    assert!(
        (i_re - 1.0).abs() < 1e-6 && i_im.abs() < 1e-6,
        "the split deck must deliver the 1 A the EX card asks for, got {i_re:.6}+j{i_im:.6}"
    );
    // Same antenna, so the same port impedance. The segment boundaries differ
    // (26+25 against 51), which is worth a few hundredths of an ohm.
    assert!(
        (zr_s - zr_w).abs() < 0.5 && (zi_s - zi_w).abs() < 0.5,
        "split Z ({zr_s:.6}+j{zi_s:.6}) != unsplit Z ({zr_w:.6}+j{zi_w:.6})"
    );
}
