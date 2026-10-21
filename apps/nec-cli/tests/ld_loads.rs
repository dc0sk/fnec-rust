use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_fnec(deck_path: &Path, workspace_root: &Path) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .arg(deck_path)
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {}: {e}", deck_path.display()));

    assert!(
        output.status.success(),
        "fnec failed for {}: {}",
        deck_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

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

/// Feedpoint Z of the same dipole with no load, SOLVED rather than pinned.
///
/// The load identities below are shifts from this baseline. They used to write
/// the baseline down as 74.242874 + j13.899516, which tied every analytic load
/// check to one solver revision: FND-156 moved the unloaded dipole by +29 Ω of
/// reactance and all three failed, though no load stamp had changed.
fn unloaded_dipole_z(workspace_root: &Path) -> (f64, f64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-ld-unloaded-{now}.nec"));
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&path, deck).expect("failed to write unloaded deck");
    let (out, _) = run_fnec(&path, workspace_root);
    let _ = fs::remove_file(&path);
    first_feedpoint_impedance(&out)
}

#[test]
fn ld_type4_changes_feedpoint_impedance() {
    // Phase-2: LD is parsed and applied.  LD type 4 (series impedance R=100, X=50)
    // stamps directly into the Z matrix at seg 26, shifting Z_RE well above free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 4 1 26 26 100.0 50.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    // Phase-2: LD is parsed — no unknown-card warning.
    assert!(
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    // The port identity: a series 100 + j50 Ω load at the feed shifts the
    // feedpoint impedance by exactly itself, in any correct method of moments.
    //
    // This used to assert 706.724 + j498.586 — fnec's own output from the broken
    // stamp, which added the load in ohms to the dimensionless Hallén matrix and
    // over-applied it 7.0x (FND-122). A pinned number cannot tell a right answer
    // from a wrong one it was measured from, so the assertion is now the physics.
    let (loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);
    let (base_r, base_x) = unloaded_dipole_z(&workspace_root);
    assert!(
        (loaded_r - base_r - 100.0).abs() < 0.2,
        "LD4 R=100 must shift Z_RE by +100 from {base_r}, got {loaded_r}"
    );
    assert!(
        (loaded_x - base_x - 50.0).abs() < 0.2,
        "LD4 X=50 must shift Z_IM by +50 from {base_x}, got {loaded_x}"
    );
}

#[test]
fn unsupported_ld_type_is_refused() {
    // FND-161: an LD type fnec does not model is refused, naming the card. It used
    // to be skipped with a warning, which solved the antenna without the load.
    let deck_path =
        std::env::temp_dir().join(format!("fnec-ld-unsupported-{}.nec", std::process::id()));
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 9 1 26 26 1.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write deck with unsupported LD type");
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&deck_path)
        .output()
        .expect("run fnec");
    let _ = fs::remove_file(&deck_path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "an unsupported LD type must be refused:\n{stderr}"
    );
    assert!(
        stderr.contains("LD 9 1 26 26") && stderr.contains("not supported"),
        "the refusal must name the card: {stderr}"
    );
}

#[test]
fn ld_type1_parallel_r_is_supported_and_changes_impedance() {
    // Phase-2: LD type 1 (parallel RLC) is parsed and applied.
    // A 1000 Ω load at the feed raises Z_RE by exactly 1000 Ω.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld1-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 1 1 26 26 1000.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    // A 1000 Ω resistive load at the feed shifts the feedpoint resistance by
    // exactly 1000 Ω. Was pinned at 7072.840 — fnec's own output through the
    // broken stamp, 7.0x over-applied (FND-122).
    let (loaded_r, _) = first_feedpoint_impedance(&loaded_out);
    let (base_r, _) = unloaded_dipole_z(&workspace_root);
    assert!(
        (loaded_r - base_r - 1000.0).abs() < 1.0,
        "LD1 R=1000 must shift Z_RE by +1000 from {base_r}, got {loaded_r}"
    );
}

#[test]
fn ld_type2_is_series_rlc_per_metre() {
    // LD 2 is series RLC per metre (FND-280): R = 10 Ω/m and L = 1 µH/m on the
    // 10.564 / 51 m feed segment are 2.0714 Ω and 0.20714 µH, X_L = 18.481 Ω.
    // The load sits at the feed, so the port identity gives the whole answer.
    // nec2c 1.3.1 reads 81.420 + j64.704 against the unloaded 79.348 + j46.223:
    // the same shift. fnec read it as a lumped 10 Ω + 1 µH (+10 + j89.22).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld2-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 2 1 26 26 10.0 1e-6 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    let (_, loaded_x) = first_feedpoint_impedance(&loaded_out);
    // Analytic, not pinned: the identity fixes the shift exactly. The old
    // assertion was fnec's own 651.8 Ω from the broken stamp — a self-consistent
    // checker that could not fail (FND-122).
    let length = 10.564 / 51.0;
    let omega_l = 2.0 * std::f64::consts::PI * 14.2e6 * 1e-6 * length;
    let (loaded_r, _) = first_feedpoint_impedance(&loaded_out);
    let (base_r, base_x) = unloaded_dipole_z(&workspace_root);
    assert!(
        (loaded_x - base_x - omega_l).abs() < 0.2,
        "LD 2 must shift Z_IM by +ωL'·Δ = {omega_l:.4} from {base_x}, got {loaded_x}"
    );
    assert!(
        (loaded_r - base_r - 10.0 * length).abs() < 0.05,
        "LD 2 must shift Z_RE by +R'·Δ = {:.4} from {base_r}, got {loaded_r}",
        10.0 * length
    );
}

#[test]
fn ld_type3_is_parallel_rlc_per_metre() {
    // LD 3 is parallel RLC per metre (FND-280): R = 10 Ω/m and C = 1 pF/m on
    // the 10.564 / 51 m feed segment are 2.0714 Ω across 0.207 pF (54 kΩ), so the
    // load is 2.0714 Ω to 0.0001 Ω. nec2c 1.3.1 reads 81.420 + j46.223 against
    // the unloaded 79.348 + j46.223. fnec read it as a lumped series 10 Ω + 1 pF:
    // −j11208 Ω.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld3-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 3 1 26 26 10.0 0.0 1e-12\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    let (loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);
    let (base_r, base_x) = unloaded_dipole_z(&workspace_root);
    let r = 10.0 * 10.564 / 51.0;
    assert!(
        (loaded_r - base_r - r).abs() < 0.05 && (loaded_x - base_x).abs() < 0.2,
        "LD 3 must shift Z by +{r:.4} + j0 from {base_r} + j{base_x}, got {loaded_r} + j{loaded_x}"
    );
}
