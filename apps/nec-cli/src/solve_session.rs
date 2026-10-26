use super::exec_profile::ExecutionMode;
use nec_model::card::Card;

use nec_solver::constants::C0;
pub(super) const CONTINUITY_REL_RESIDUAL_MAX: f64 = 1e-3;
pub(super) const SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT: f64 = 1e-2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SolverMode {
    Hallen,
    Pulse,
    Continuity,
    Sinusoidal,
    /// Mixed-potential EFIE (triangle basis) — the opt-in second solver
    /// (PH9-CHK-007). Reaches degree-3 junctions, closed loops, and near-ground
    /// currents (Sommerfeld) that the Hallén basis cannot represent.
    Mpie,
}

impl SolverMode {
    /// The geometry this mode solves (FND-227): refined at free ends for the Hallén
    /// family, the deck's own for the MPIE and the experimental bases, which do not
    /// take the refinement. Exhaustive, so a new mode has to choose.
    pub(super) fn geometry(
        self,
        deck: &nec_model::deck::NecDeck,
    ) -> Result<Vec<Segment>, nec_solver::GeometryError> {
        match self {
            SolverMode::Hallen | SolverMode::Sinusoidal => nec_solver::build_geometry(deck),
            SolverMode::Pulse | SolverMode::Continuity | SolverMode::Mpie => {
                nec_solver::build_deck_geometry(deck)
            }
        }
    }

    /// Every mode, in the order the CLI lists them. The one enumeration of the
    /// `--solver` axis: parsing and both error messages derive from it, and a
    /// test ties the usage line to it (FND-148). A new variant cannot be left
    /// out — see `every_solver_mode_is_listed_once`.
    pub(super) const ALL: [SolverMode; 5] = [
        SolverMode::Hallen,
        SolverMode::Pulse,
        SolverMode::Continuity,
        SolverMode::Sinusoidal,
        SolverMode::Mpie,
    ];

    /// The `--solver` values joined for a message: `hallen|pulse|…`.
    pub(super) fn flag_alternation() -> String {
        Self::ALL.map(Self::as_flag).join("|")
    }

    /// The mode a `--solver` value selects.
    pub(super) fn from_flag(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_flag() == value)
    }

    /// The `--solver` value that selects this mode.
    ///
    /// So a message about the flag can name what the user actually typed rather
    /// than a `Debug` spelling that differs from it.
    pub(super) fn as_flag(self) -> &'static str {
        match self {
            SolverMode::Hallen => "hallen",
            SolverMode::Pulse => "pulse",
            SolverMode::Continuity => "continuity",
            SolverMode::Sinusoidal => "sinusoidal",
            SolverMode::Mpie => "mpie",
        }
    }
}

impl SolverMode {
    /// The mode's name in reports and benchmark records — its flag value, so
    /// the two spellings cannot drift (they were two identical matches).
    pub(super) fn as_str(self) -> &'static str {
        self.as_flag()
    }
}

/// Near-ground impedance model (PH9-CHK-006). `Rcm` is the default scalar-Γ
/// reflection-coefficient image; `Sommerfeld` adds the surface-wave correction for
/// straight horizontal wires (accurate below ~0.1 λ, = nec2c GN2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GroundSolver {
    Rcm,
    Sommerfeld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PulseRhsMode {
    Raw,
    Nec2,
}

impl PulseRhsMode {
    pub(super) fn as_contract_str(self) -> &'static str {
        match self {
            PulseRhsMode::Raw => "Raw",
            PulseRhsMode::Nec2 => "Nec2",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BenchRecord {
    pub(super) mode: String,
    pub(super) pulse_rhs: String,
    pub(super) exec: String,
    pub(super) freq_mhz: f64,
    /// `None` where the route computes no residual: printed `n/a` on the diag
    /// line and the bench CSV, `null` in bench JSON — never a perfect-looking 0
    /// (FND-306, D8).
    pub(super) abs_res: Option<f64>,
    pub(super) rel_res: Option<f64>,
    /// `None` only for a `--hosts` record: the worker does not send it.
    pub(super) diag_spread: Option<f64>,
    /// The sinusoidal basis's own (pre-fallback) residual; `None` when that basis
    /// did not run (D8: the old 0 read as a perfect sinusoidal solve).
    pub(super) sin_rel_res: Option<f64>,
}

/// A residual for the diag line and the bench CSV: `{:.6e}`, or `n/a` when the
/// route computed none (FND-306).
pub(super) fn residual_field(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_string(), |x| format!("{x:.6e}"))
}

/// The same for bench JSON: a number, or `null`.
pub(super) fn residual_json(v: Option<f64>) -> String {
    v.map_or_else(|| "null".to_string(), |x| format!("{x:.6e}"))
}
use nec_report::{
    render_text_report, CurrentRow, FeedpointRow, LoadRow, PatternRow, PointReport, SourceRow,
};
// The plane-wave builders and path solvers are deliberately absent: the CLI no
// longer assembles a plane-wave system itself, it asks
// `nec_solver::solve_hallen_planewave_routed`. Re-adding any of them here is how
// a sixth copy of the routing decision would start, so it should be visible in a
// diff (FND-128).
use nec_solver::{
    assemble_pocklington_matrix, assemble_z_matrix_with_ground, build_hallen_rhs,
    compute_radiation_pattern, integrate_radiated_power, scale_excitation_for_pulse_rhs, solve,
    solve_hallen, solve_hallen_sinusoidal_basis, solve_with_continuity_basis_per_wire,
    FarFieldPoint, GroundModel, Segment, ZMatrix,
};
use num_complex::Complex64;

/// How a command-line user reaches the MPIE, for diagnostics that recommend it.
///
/// One constant rather than a literal at each call site: a GUI names its picker
/// instead, and the whole point of the caller-supplied remedy is that neither
/// frontend quotes the other's interface.
pub(crate) const CLI_MPIE_REMEDY: &str = "re-run with `--solver mpie`";

/// Who prints a point's geometry caveats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeometryCaveats {
    /// This point prints them — a single solve, and each point of an `FR` sweep,
    /// whose low-ground caveat differs with the frequency.
    PerPoint,
    /// The caller prints them once, in its own terms: `sweep --resonance`, whose
    /// probes are not points the user asked for, and whose remedies differ
    /// (FND-218). Printing them per probe repeated the caveat once per probe
    /// (12 times in an 11-iteration search), with a remedy the search refuses.
    ShownByCaller,
}

/// How a command-line user asks for the Sommerfeld surface wave — offered by the
/// low-ground caveat on the main command only, where the flag exists (FND-217).
pub(crate) const CLI_SOMMERFELD_REMEDY: &str = "re-run with `--ground-solver sommerfeld`";

/// **Dormant (FND-130).** The pulse solver's EX 4 current-source path. No input
/// reaches it: an `EX 4` deck with any solver but Hallén is refused before the
/// solve (`--solver hallen` only, corpus-pinned by
/// `dipole-ex4-pulse-current-freesp-51seg`), so this finds no current source to
/// act on. Retained rather than deleted because the pulse bases are kept, behind
/// `--experimental-solver`, for experiment (maintainer, 2026-09-26, FND-080);
/// lifting that refusal is the one change that would wake it, and it would then
/// need the validation it never had — its only reference value was
/// -345.6 - j988.0 Ω against Hallén's 74 Ω.
pub(super) struct PulseCurrentSourceConstraint {
    pub(super) seg_index: usize,
    pub(super) source_current: Complex64,
    pub(super) original_row: Vec<Complex64>,
}

/// A CPU far-field result as a pattern-table row, its fields kept.
fn pattern_row(r: &nec_solver::farfield::FarFieldResult) -> PatternRow {
    PatternRow {
        theta_deg: r.theta_deg,
        phi_deg: r.phi_deg,
        gain_total_dbi: r.gain_total_dbi,
        gain_theta_dbi: r.gain_theta_dbi,
        gain_phi_dbi: r.gain_phi_dbi,
        axial_ratio: r.axial_ratio,
        e_field: Some((r.e_theta, r.e_phi)),
        ellipse: Some(ellipse(r.e_theta, r.e_phi)),
    }
}

/// NEC-2's polarization ellipse of a far-field point, in the report's terms.
fn ellipse(e_theta: num_complex::Complex64, e_phi: num_complex::Complex64) -> nec_report::Ellipse {
    use nec_solver::farfield::PolarizationSense as S;
    let e = nec_solver::polarization_ellipse(e_theta, e_phi);
    nec_report::Ellipse {
        axial_ratio: e.axial_ratio,
        tilt_deg: e.tilt_deg,
        sense: match e.sense {
            S::None => nec_report::Sense::None,
            S::Linear => nec_report::Sense::Linear,
            S::Right => nec_report::Sense::Right,
            S::Left => nec_report::Sense::Left,
        },
    }
}

/// What a frequency point prints: the local solve's tables, or the text a remote
/// worker's result was formatted to (the worker sends a feedpoint, not tables).
pub(super) enum PointOutput {
    Tables(Box<PointReport>),
    Text(String),
}

impl PointOutput {
    /// The point's text report.
    pub(super) fn text(&self) -> String {
        match self {
            PointOutput::Tables(point) => render_text_report(&nec_report::ReportInput {
                build: Some(crate::VERSION),
                ..point.as_input()
            }),
            PointOutput::Text(text) => text.clone(),
        }
    }
}

pub(super) struct FrequencySolveResult {
    pub(super) report: PointOutput,
    pub(super) diag_line: String,
    pub(super) bench: BenchRecord,
    pub(super) sweep_summary: Option<SweepPointSummary>,
    /// This point's negative-resistance sentences, returned rather than printed
    /// so a sweep can report them once (FND-069).
    pub(super) negative_r: Vec<String>,
    /// The lowest feedpoint resistance at this point, for the sweep aggregate;
    /// `None` when nothing is priced (a receive deck).
    pub(super) min_feed_re: Option<f64>,
    /// Whether the device solved this point (a GPU sweep reports the count).
    pub(super) ran_on_gpu: bool,
    /// Whether the surface-wave correction reached this point's impedance, so the
    /// run can say once that V and I are not the corrected solve's (FND-286).
    pub(super) surface_wave_applied: bool,
    /// What this point's resolved loads were, for the sweep's aggregate caveat
    /// (FND-209): the deck alone cannot say.
    pub(super) run_loads: nec_solver::validate::RunLoads,
    /// The relative residual of this point's f64 Hallén solve, for
    /// `validate::residual_caveat_at` (FND-302); `None` for routes that compute
    /// none and for every residual whose `ResidualKind` is not judged: a device
    /// (f32) solve (judged by the device at 1e-4), the sinusoidal projection
    /// (judged by `--sin-fallback-rel-max`) and the pulse and continuity bases
    /// (printed, not judged).
    pub(super) hallen_rel_res: Option<f64>,
}

/// The `exec` a point actually ran: `gpu` only when the device solved it.
pub(super) fn exec_label(mode: ExecutionMode, ran_on_gpu: bool) -> &'static str {
    match (mode, ran_on_gpu) {
        (ExecutionMode::Gpu, true) => "gpu",
        (ExecutionMode::Gpu, false) => "gpu(cpu-fallback)",
        (mode, _) => mode.as_cli_str(),
    }
}

pub(super) struct SweepPointSummary {
    pub(super) freq_mhz: f64,
    pub(super) tag: usize,
    pub(super) seg: usize,
    pub(super) z_re: f64,
    pub(super) z_im: f64,
    /// The unvalidated-solver caveat, when the point came from one (FND-080);
    /// the JSON record carries it so it travels with the numbers.
    pub(super) caveat: Option<&'static str>,
    /// Every feedpoint of the point, in the text report's FEEDPOINTS order; the
    /// fields above are its first element. The JSON record carries the whole list
    /// (FND-291): it used to carry the first alone, so a two-source deck lost its
    /// second row with no marker.
    pub(super) feedpoints: Vec<SweepFeedpoint>,
}

/// One feedpoint of a [`SweepPointSummary`].
pub(super) struct SweepFeedpoint {
    pub(super) tag: usize,
    pub(super) seg: usize,
    pub(super) z_re: f64,
    pub(super) z_im: f64,
}

pub(super) fn l2_norm(v: &[Complex64]) -> f64 {
    v.iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt()
}

pub(super) fn matrix_diagonal_spread(z: &ZMatrix) -> f64 {
    if z.n == 0 {
        return 0.0;
    }

    let mut max_diag = 0.0f64;
    let mut min_diag = f64::INFINITY;
    for i in 0..z.n {
        let d = z.get(i, i).norm();
        max_diag = max_diag.max(d);
        min_diag = min_diag.min(d);
    }

    if !max_diag.is_finite() || !min_diag.is_finite() {
        return f64::NAN;
    }
    if max_diag == 0.0 {
        return 0.0;
    }

    max_diag / min_diag.max(1e-30)
}

pub(super) fn residual_zi_minus_v(
    z: &ZMatrix,
    i_vec: &[Complex64],
    v_vec: &[Complex64],
) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];
    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        r[row] = zi - v_vec[row];
    }

    let res = l2_norm(&r);
    let denom = l2_norm(v_vec);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

/// True if the deck carries an incident-plane-wave EX card (NEC2 types 1/2/3).
pub(super) fn deck_has_plane_wave(deck: &nec_model::deck::NecDeck) -> bool {
    deck.cards.iter().any(|c| match c {
        Card::Ex(ex) => ex.kind().is_plane_wave(),
        _ => false,
    })
}

/// Incident-plane-wave receive-pattern sweep (PH9-CHK-001).
///
/// The plane-wave EX card's `tag` = NTHETA, `segment` = NPHI, `voltage_real`/
/// `voltage_imag` = θ0/φ0, and `theta_inc`/`phi_inc` = Δθ/Δφ define a grid of
/// incidence directions. For each direction the receiving antenna is solved and
/// the peak induced current recorded; the peak current tracks the transmit gain
/// pattern by reciprocity, so the normalized response (dB, 0 at the sweep peak) is
/// the receive pattern. Returns an empty vector for a single incidence
/// (NTHETA·NPHI ≤ 1).
fn plane_wave_receive_sweep(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
    ground: &GroundModel,
    loads: &[Complex64],
) -> Result<Vec<nec_report::ReceivePatternRow>, String> {
    let ex = deck
        .cards
        .iter()
        .find_map(|c| match c {
            Card::Ex(e) if e.kind().is_plane_wave() => Some(e.clone()),
            _ => None,
        })
        .ok_or("no plane-wave EX card for receive sweep")?;
    let n_theta = ex.tag.max(1);
    let n_phi = ex.segment.max(1);
    if n_theta * n_phi <= 1 {
        return Ok(Vec::new());
    }

    // The paths, section layout and corner term do not depend on the direction;
    // built per direction they made a 2701-point pattern 44× slower (FND-162).
    // Over the matrix's own ground: the corner terms take its images and the
    // incident field its reflected wave (FND-170).
    // The loads in full — deck LD and --loads-config — so the plan builds its own
    // loaded matrix and never reads `z_mat`'s stamps (FND-197).
    let plan = nec_solver::plan_hallen_planewave(segs, freq_hz, ground, loads);
    let mut raw: Vec<(f64, f64, f64)> = Vec::new(); // (θ, φ, peak|I|)
    for it in 0..n_theta {
        for ip in 0..n_phi {
            let theta = ex.voltage_real + it as f64 * ex.theta_inc;
            let phi = ex.voltage_imag + ip as f64 * ex.phi_inc;
            // Single-incidence deck at this arrival direction.
            let mut d = deck.clone();
            for c in &mut d.cards {
                if let Card::Ex(e) = c {
                    if e.kind().is_plane_wave() {
                        e.tag = 1;
                        e.segment = 1;
                        e.voltage_real = theta;
                        e.voltage_imag = phi;
                        e.theta_inc = 0.0;
                        e.phi_inc = 0.0;
                    }
                }
            }
            // The one copy of the plane-wave routing decision, shared with
            // `solve_hallen_routed`'s own arm. It borrows the matrix, so calling
            // it once per direction cannot re-stamp the load columns (FND-128).
            let currents =
                nec_solver::solve_hallen_planewave_planned(&d, segs, z_mat, freq_hz, &plan)
                    .map_err(|e| e.to_string())?;
            // Before the reduction, not after. `f64::max` returns the OTHER
            // operand when one side is NaN, so `fold(0.0, f64::max)` turns a
            // fully diverged solve into exactly 0.0 — printed as a -999.99 dB
            // null indistinguishable from a real one (FND-127). Refusing here
            // means the laundering has nothing to launder.
            nec_solver::check_currents_finite(&currents).map_err(|e| e.to_string())?;
            let peak = currents.iter().map(|c| c.norm()).fold(0.0f64, f64::max);
            raw.push((theta, phi, peak));
        }
    }
    let max_peak = raw.iter().map(|r| r.2).fold(0.0f64, f64::max);
    Ok(raw
        .into_iter()
        .map(|(theta, phi, peak)| nec_report::ReceivePatternRow {
            theta_deg: theta,
            phi_deg: phi,
            response_db: if peak > 0.0 && max_peak > 0.0 {
                20.0 * (peak / max_peak).log10()
            } else {
                -999.99
            },
        })
        .collect())
}

/// True if the deck carries a current-source EX card (NEC2 type 4).
pub(super) fn deck_has_current_source(deck: &nec_model::deck::NecDeck) -> bool {
    deck.cards.iter().any(|c| match c {
        Card::Ex(ex) => ex.kind() == nec_model::card::ExcitationKind::CurrentSource,
        _ => false,
    })
}

/// Dormant — see [`PulseCurrentSourceConstraint`].
pub(super) fn collect_pulse_current_source_constraints(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
) -> Result<Vec<PulseCurrentSourceConstraint>, String> {
    let mut out = Vec::new();

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        // NEC2 numbering: the current source is type 4. (This staged path is
        // reached only once the current-source runtime semantics land; today
        // build_excitation rejects non-0 types before the solve.)
        if ex.kind() != nec_model::card::ExcitationKind::CurrentSource {
            continue;
        }

        let seg_index = nec_solver::find_deck_segment(segs, ex.tag, ex.segment)
            .ok_or_else(|| nec_solver::missing_reference("EX", ex.tag, ex.segment, segs))?;

        out.push(PulseCurrentSourceConstraint {
            seg_index,
            source_current: Complex64::new(ex.voltage_real, ex.voltage_imag),
            original_row: Vec::new(),
        });
    }

    Ok(out)
}

/// Dormant — see [`PulseCurrentSourceConstraint`].
pub(super) fn apply_pulse_current_source_constraints(
    z_mat: &mut ZMatrix,
    rhs: &mut [Complex64],
    constraints: &mut [PulseCurrentSourceConstraint],
) {
    for constraint in constraints {
        constraint.original_row = (0..z_mat.n)
            .map(|col| z_mat.get(constraint.seg_index, col))
            .collect();

        let mut replacement_row = vec![Complex64::new(0.0, 0.0); z_mat.n];
        replacement_row[constraint.seg_index] = Complex64::new(1.0, 0.0);
        z_mat.replace_row(constraint.seg_index, &replacement_row);
        rhs[constraint.seg_index] = constraint.source_current;
    }
}

/// Dormant — see [`PulseCurrentSourceConstraint`].
pub(super) fn pulse_current_source_voltage(
    constraint: &PulseCurrentSourceConstraint,
    i_vec: &[Complex64],
    seg_length: f64,
    freq_hz: f64,
) -> Complex64 {
    let impressed_field: Complex64 = constraint
        .original_row
        .iter()
        .zip(i_vec.iter())
        .map(|(z, i)| *z * *i)
        .sum();
    -(impressed_field * seg_length * (C0 / freq_hz))
}

/// Expand an `NE`/`NH` observation grid into Cartesian points (PH9-CHK-004).
///
/// `coord_type = 0` is a rectangular `NX×NY×NZ` grid over `(x0,y0,z0)` with steps
/// `(dx,dy,dz)`. `coord_type ≠ 0` is a **spherical** grid (NEC-2 `I1 = 1`): the
/// fields are reinterpreted as `NX→R (r0=x0, Δr=dx)`, `NY→φ (φ0=y0°, Δφ=dy°)`,
/// `NZ→θ (θ0=z0°, Δθ=dz°)` with `θ` measured from `+z`, and each point maps to
/// `(r sinθ cosφ, r sinθ sinφ, r cosθ)`. The nesting (θ outer, φ middle, R inner)
/// matches nec2c's point order. The field is then evaluated at the Cartesian
/// point, so it is consistent with the rectangular path.
#[allow(clippy::too_many_arguments)]
fn near_field_grid_points(
    coord_type: u32,
    nx: u32,
    ny: u32,
    nz: u32,
    x0: f64,
    y0: f64,
    z0: f64,
    dx: f64,
    dy: f64,
    dz: f64,
) -> Vec<nec_solver::NearFieldPoint> {
    let mut points = Vec::new();
    if coord_type == 0 {
        for ix in 0..nx.max(1) {
            for iy in 0..ny.max(1) {
                for iz in 0..nz.max(1) {
                    points.push(nec_solver::NearFieldPoint {
                        x: x0 + ix as f64 * dx,
                        y: y0 + iy as f64 * dy,
                        z: z0 + iz as f64 * dz,
                    });
                }
            }
        }
    } else {
        // Spherical: R = nx, φ = ny, θ = nz; nec2c orders θ outer, φ mid, R inner.
        for it in 0..nz.max(1) {
            let theta = (z0 + it as f64 * dz).to_radians();
            for ip in 0..ny.max(1) {
                let phi = (y0 + ip as f64 * dy).to_radians();
                for ir in 0..nx.max(1) {
                    let r = x0 + ir as f64 * dx;
                    points.push(nec_solver::NearFieldPoint {
                        x: r * theta.sin() * phi.cos(),
                        y: r * theta.sin() * phi.sin(),
                        z: r * theta.cos(),
                    });
                }
            }
        }
    }
    points
}

/// PH9-CHK-004: compute the near electric field for every `NE` card in the deck.
///
/// Each `NE` card defines an `NX×NY×NZ` grid of observation points (rectangular or
/// spherical — see [`near_field_grid_points`]); the near field is the
/// Hertzian-element sum over the solved segment currents
/// (`nec_solver::near_e_field`), validated to match the far field at large range.
fn build_near_field_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    i_vec: &[Complex64],
    freq_hz: f64,
    ground: &GroundModel,
) -> Vec<nec_report::NearFieldRow> {
    let mut points: Vec<nec_solver::NearFieldPoint> = Vec::new();
    for card in &deck.cards {
        let Card::Ne(ne) = card else { continue };
        points.extend(near_field_grid_points(
            ne.coord_type,
            ne.nx,
            ne.ny,
            ne.nz,
            ne.x0,
            ne.y0,
            ne.z0,
            ne.dx,
            ne.dy,
            ne.dz,
        ));
    }
    if points.is_empty() {
        return Vec::new();
    }
    nec_solver::near_e_field(segs, i_vec, freq_hz, &points, ground)
        .into_iter()
        .map(|f| nec_report::NearFieldRow {
            x: f.x,
            y: f.y,
            z: f.z,
            ex: f.e[0],
            ey: f.e[1],
            ez: f.e[2],
        })
        .collect()
}

/// PH9-CHK-004: compute the near magnetic field for every `NH` card in the deck
/// (the magnetic companion to [`build_near_field_rows`]).
fn build_near_h_field_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    i_vec: &[Complex64],
    freq_hz: f64,
    ground: &GroundModel,
) -> Vec<nec_report::NearHFieldRow> {
    let mut points: Vec<nec_solver::NearFieldPoint> = Vec::new();
    for card in &deck.cards {
        let Card::Nh(nh) = card else { continue };
        points.extend(near_field_grid_points(
            nh.coord_type,
            nh.nx,
            nh.ny,
            nh.nz,
            nh.x0,
            nh.y0,
            nh.z0,
            nh.dx,
            nh.dy,
            nh.dz,
        ));
    }
    if points.is_empty() {
        return Vec::new();
    }
    nec_solver::near_h_field(segs, i_vec, freq_hz, &points, ground)
        .into_iter()
        .map(|f| nec_report::NearHFieldRow {
            x: f.x,
            y: f.y,
            z: f.z,
            hx: f.h[0],
            hy: f.h[1],
            hz: f.h[2],
        })
        .collect()
}

/// The `PT` card's current-print control, as NEC-2 reads it (PH9-CHK-004, FND-299):
/// `I1 ≤ −1` suppresses the CURRENTS table; `I1 ≥ 0` with `I3 = 0` prints every
/// segment (nec2c sets its flag to "print all" there, whatever I1 and I2 say);
/// `I1 ≥ 0` with a range prints the segments `(I2, I3..I4)` selects — a tag's
/// segments, or absolute segment numbers when `I2` is 0 — through the one range
/// resolver (`deck_segment_in_range`, FND-282). `I1 = 1..3` select nec2c's
/// receive-pattern print formats; fnec prints the same rows in its own table and
/// the parser warns. The last `PT` card wins, as in nec2c.
///
/// The selection is made on the geometry's segments, before the rows are built:
/// a row carries only `(tag, tag_index)`, which cannot answer an absolute range.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PtSelection {
    All,
    Nothing,
    Range { tag: u32, first: u32, last: u32 },
}

impl PtSelection {
    fn of(deck: &nec_model::deck::NecDeck) -> Self {
        let Some(pt) = deck.cards.iter().rev().find_map(|c| match c {
            Card::Pt(p) => Some(p),
            _ => None,
        }) else {
            return PtSelection::All;
        };
        if pt.mode <= -1 {
            return PtSelection::Nothing;
        }
        if pt.seg_first == 0 {
            return PtSelection::All;
        }
        PtSelection::Range {
            tag: pt.tag,
            first: pt.seg_first,
            last: pt.seg_last,
        }
    }

    fn prints(self, seg: &Segment) -> bool {
        match self {
            PtSelection::All => true,
            PtSelection::Nothing => false,
            PtSelection::Range { tag, first, last } => {
                nec_solver::deck_segment_in_range(seg, tag, first, last)
            }
        }
    }
}

/// PH9-CHK-005: a passive antenna cannot have a negative input resistance, so a
/// negative `Re(Z)` means the reported result is unreliable.
///
/// The Hallén-basis diagnosis moved to [`nec_solver::validate::negative_resistance_warning`]
/// in FND-014 — it was private here and reachable from one call site, so the GUI,
/// the Python bindings and the worker reported a physically impossible impedance
/// with no caveat at all.
///
/// The MPIE arm deliberately stayed behind. Its message is a claim about *this
/// binary's* solver arsenal — that the MPIE has no documented negative-`R` case,
/// so one is a defect rather than a modelling limitation — not a property of the
/// deck, and `SolverMode` is CLI-private precisely because the other frontends are
/// Hallén-only. If the GUI ever gains an MPIE picker, the compiler will require
/// that decision to be made explicitly rather than a `false` silently travelling.
///
/// Still skipped for `pulse`/`continuity`/`sinusoidal`, whose current-source corpus
/// has documented negative-`R` values.
/// The negative-resistance caveat for a whole run (FND-069).
///
/// A single frequency keeps the per-point sentence, which names the segment and
/// its `Re Z`. A sweep gets ONE line: the per-point sentence embeds `Re Z`, so
/// every point's text differs and a 50-point sweep over a junctioned deck printed
/// 50 lines — the GUI and `fnec_py` aggregated through the shared producer, and
/// this frontend did not. The local and the distributed sweep both come here.
///
/// `per_point` holds each point's own sentences and `min_feed_re` its lowest
/// feedpoint resistance, both in frequency order.
pub(super) fn run_negative_resistance_warnings(
    per_point: Vec<Vec<String>>,
    min_feed_re: &[Option<f64>],
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    solver_mode: SolverMode,
    run_loads: nec_solver::validate::RunLoads,
) -> Vec<String> {
    if per_point.len() <= 1 {
        return per_point.into_iter().flatten().collect();
    }
    let z_res: Vec<f64> = min_feed_re.iter().filter_map(|z| *z).collect();
    match solver_ctx(solver_mode) {
        Some(ctx) => nec_solver::validate::swept_negative_resistance_caveat(
            &z_res,
            deck,
            segs,
            ctx.with_loads(run_loads),
        )
        .into_iter()
        .collect(),
        None => {
            let n = z_res
                .iter()
                .filter(|z| nec_solver::validate::is_negative_resistance(**z))
                .count();
            if n == 0 {
                return Vec::new();
            }
            vec![format!(
                "{n} of {} sweep points report negative feedpoint resistance, which is \
                 physically impossible — the output of an unvalidated solver (FND-080)",
                z_res.len()
            )]
        }
    }
}

/// The shared validator's context for a mode; `None` for the pulse bases, whose
/// negative resistance is the expected output of an unvalidated solver and gets
/// its own wording (FND-080).
fn solver_ctx(solver_mode: SolverMode) -> Option<nec_solver::validate::SolverContext<'static>> {
    match solver_mode {
        SolverMode::Hallen => Some(nec_solver::validate::SolverContext::cli_hallen()),
        SolverMode::Mpie => Some(nec_solver::validate::SolverContext {
            kind: nec_solver::validate::SolverKind::Mpie,
            mpie_remedy: CLI_MPIE_REMEDY,
            sommerfeld_remedy: None,
            loads: nec_solver::validate::RunLoads::NONE,
        }),
        // FND-081: sinusoidal is Hallén's matrix in a projected basis and as
        // accurate, so it takes Hallén's wording.
        SolverMode::Sinusoidal => Some(nec_solver::validate::SolverContext::cli_hallen()),
        SolverMode::Pulse | SolverMode::Continuity => None,
    }
}

/// Split from the emission so the mode routing is testable without a deck that
/// actually produces a negative resistance.
pub(super) fn negative_resistance_warnings(
    rows: &[FeedpointRow],
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    solver_mode: SolverMode,
    run_loads: nec_solver::validate::RunLoads,
) -> Vec<String> {
    // `negative_resistance_cause` takes the solver context and picks the cause
    // itself, so the GUI's MPIE runs get the same "report it as a solver defect"
    // wording. FND-081: every mode reports a negative resistance, which is
    // physically impossible whatever produced it; the pulse bases get their own
    // wording, since for them it is the expected output of an unvalidated solver.
    let ctx = match solver_ctx(solver_mode) {
        Some(ctx) => ctx.with_loads(run_loads),
        None => {
            return rows
                .iter()
                .filter(|r| r.z_in.re < 0.0)
                .map(|r| {
                    format!(
                        "tag {} seg {}: negative feedpoint resistance ({:.3} ohm), which is \
                         physically impossible — the output of an unvalidated solver (FND-080)",
                        r.tag, r.seg, r.z_in.re
                    )
                })
                .collect();
        }
    };
    rows.iter()
        .filter_map(|r| {
            nec_solver::validate::negative_resistance_warning(
                r.z_in.re, r.tag, r.seg, deck, segs, ctx,
            )
        })
        .collect()
}

/// Whether the opt-in Sommerfeld surface-wave correction (PH9-CHK-006) actually
/// reached the reported feedpoint impedance. The distinction matters for honest
/// diagnostics: `--ground-solver sommerfeld` is silently declined for geometry the
/// correction does not cover, and a user who asked for the surface wave and did
/// not get it must be told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SommerfeldOutcome {
    /// Not selected, or not applicable to this solver / ground model.
    NotRequested,
    /// Selected and applied — the reported `Z` includes the surface wave.
    Applied,
    /// Selected over finite ground but declined by the geometry (bent or mixed
    /// wire directions); the reported `Z` is the unchanged scalar-Γ (`rcm`) value.
    Declined,
    /// Selected over finite ground on a plane-wave receive deck: the correction
    /// is a feedpoint-impedance delta, and a receive solve has no feedpoint, so
    /// its currents are the reflection-coefficient result (FND-170).
    DeclinedReceive,
}

/// PH9-CHK-006: `--ground-solver sommerfeld` covers straight wires only, and used
/// to decline everything else in silence — leaving the user believing they had the
/// surface wave when they had the reflection-coefficient result. Say so.
fn warn_if_sommerfeld_declined(outcome: SommerfeldOutcome) {
    if outcome == SommerfeldOutcome::DeclinedReceive {
        eprintln!(
            "warning: --ground-solver sommerfeld corrects a feedpoint impedance, and a \
             plane-wave receive solve has none; the induced currents use the \
             reflection-coefficient ground model (rcm) — the matrix's normal-incidence \
             image and the wave's own Fresnel reflection (FND-170)"
        );
    }
    if outcome == SommerfeldOutcome::Declined {
        eprintln!(
            "warning: --ground-solver sommerfeld covers a single straight wire (collinear \
             segments, horizontal, vertical or tilted) with one feed and no TL/NT network, \
             and declined this deck (bent, mixed or parallel wires, several feeds, or a \
             network — FND-206, FND-208); the reported feedpoint impedance is the unchanged \
             reflection-coefficient (rcm) result. For the surface \
             wave on bent geometry use --solver mpie, which assembles the reflected kernels \
             into its Z-matrix (PH9-CHK-006 / PH9-CHK-007)"
        );
    }
}

/// Whether this run asks for the surface-wave correction (PH9-CHK-006), and over
/// which ground: `--ground-solver sommerfeld`, a finite ground, and not the MPIE —
/// which assembles the reflected kernels into its own Z-matrix, so the delta would
/// count the surface wave twice. The one copy: the feedpoint rows apply the
/// correction on it and the NEC-2D report refuses on it (FND-286), so the two
/// cannot disagree about what was requested.
pub(super) fn sommerfeld_requested(
    solver_mode: SolverMode,
    ground_solver: GroundSolver,
    ground: &GroundModel,
) -> Option<(f64, f64)> {
    match (solver_mode, ground_solver, ground) {
        (SolverMode::Mpie, _, _) => None,
        (_, GroundSolver::Sommerfeld, GroundModel::SimpleFiniteGround { eps_r, sigma }) => {
            Some((*eps_r, *sigma))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)] // cohesive feedpoint inputs; splitting would obscure
pub(super) fn build_feedpoint_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    pulse_current_sources: &[PulseCurrentSourceConstraint],
    solver_mode: SolverMode,
    current_source_port: Option<Complex64>,
    freq_hz: f64,
    ground: &GroundModel,
    ground_solver: GroundSolver,
) -> Result<(Vec<FeedpointRow>, SommerfeldOutcome), String> {
    let mut rows = Vec::new();

    // PH9-CHK-006: precompute the Sommerfeld surface-wave ΔZ correction inputs once
    // (applied per feedpoint below) when the user selects the sommerfeld ground
    // solver over finite ground. This delta upgrades the *Hallén + scalar-Γ* result
    // to the Sommerfeld impedance; the MPIE path already assembles the exact
    // reflected (Sommerfeld) kernels into its Z-matrix, so applying the delta there
    // would double-count the surface wave — skip it for `--solver mpie`.
    let sommerfeld_ground = sommerfeld_requested(solver_mode, ground_solver, ground);
    // The correction is a one-port quantity: ⟨J, ΔG J⟩ / I_feed² over the whole
    // structure's currents. With several feeds that charges every port with the
    // whole structure's reaction — a symmetric pair got 2(ΔZ11 ± ΔZ12), 76.29 −
    // j65.29 against nec2c's 75.01 − j30.15 (FND-206) — and with a TL/NT network
    // the feed current carries the network branch, which is not antenna current
    // (FND-208). Both decline, and say so, rather than correct wrongly. The test is
    // the one the low-ground caveat names the remedy on, so the two cannot drift
    // (FND-217); the geometry half is also what `ground_z_correction` checks.
    let sommerfeld_applicable =
        sommerfeld_ground.is_some() && nec_solver::sommerfeld::correction_applies(deck, segs);
    let mut sommerfeld_outcome = if sommerfeld_ground.is_some() {
        // Downgraded to `Applied` only if a correction actually comes back.
        SommerfeldOutcome::Declined
    } else {
        SommerfeldOutcome::NotRequested
    };
    let midpoints: Vec<[f64; 3]> = segs.iter().map(|s| s.midpoint).collect();
    let directions: Vec<[f64; 3]> = segs.iter().map(|s| s.direction).collect();
    let lengths: Vec<f64> = segs.iter().map(|s| s.length).collect();

    // Through the shared seam (FND-031), which excludes plane waves — their
    // tag/segment fields carry NTHETA/NPHI, not a driven segment — while keeping
    // current sources, which ARE feedpoints here: the CLI prices one from the
    // solved port voltage, corpus-pinned under PH8-CHK-001 (`dipole-ex4-freesp-51seg`).
    // A seam that filtered on "voltage source" would have deleted that row.
    for (ex, role) in nec_solver::feedpoints(deck) {
        let Some((idx, seg)) =
            nec_solver::find_deck_segment(segs, ex.tag, ex.segment).map(|i| (i, &segs[i]))
        else {
            continue;
        };

        let current = i_vec[idx];
        // Through the shared seam, so this column and the GUI's gain correction
        // cannot disagree about what a feedpoint's voltage is — they did, and the
        // GUI lost 5.78 dB on every current-source deck over lossy ground
        // (FND-114). Sabotaging that one function now moves BOTH.
        let v_source =
            nec_solver::feedpoint_drive_voltage(role, seg.length, v_vec[idx], current_source_port)
                .unwrap_or_else(|| {
                    // Reached only for a current source with no solved port voltage. The
                    // dormant pulse path is retained here rather than deleted: EX 4 with
                    // any non-Hallén solver is refused at function entry and that refusal
                    // is corpus-pinned, so this is unreachable today (FND-130).
                    if matches!(solver_mode, SolverMode::Pulse) {
                        pulse_current_sources
                            .iter()
                            .find(|constraint| constraint.seg_index == idx)
                            .map(|constraint| {
                                pulse_current_source_voltage(constraint, i_vec, seg.length, freq_hz)
                            })
                            .unwrap_or(v_vec[idx] * seg.length)
                    } else {
                        v_vec[idx] * seg.length
                    }
                });
        // The shared seam: a zero current has no impedance, and printing the
        // source voltage instead was reporting a different quantity in the units
        // of the one asked for (FND-050/058).
        let mut z_in = nec_solver::feedpoint_impedance(
            v_source,
            current,
            ex.tag as usize,
            ex.segment as usize,
        )
        .map_err(|e| e.to_string())?;

        // PH9-CHK-006: add the Sommerfeld surface-wave correction to the near-ground
        // feedpoint impedance for any straight wire — horizontal, vertical, or tilted
        // (bent / mixed geometry is declined and keeps the scalar-Γ result).
        if let Some((eps_r, sigma)) = sommerfeld_ground.filter(|_| sommerfeld_applicable) {
            if let Some(dz) = nec_solver::sommerfeld::ground_z_correction(
                &midpoints,
                &directions,
                &lengths,
                i_vec,
                idx,
                freq_hz,
                eps_r,
                sigma,
            ) {
                z_in += dz;
                sommerfeld_outcome = SommerfeldOutcome::Applied;
            }
        }

        rows.push(FeedpointRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            v_source,
            current,
            z_in,
        });
    }

    // A deck with no feedpoint never asked the correction anything. A plane-wave
    // receive deck over finite ground did get a ground — without the surface wave
    // the user asked for — so it is a decline of its own kind and says so.
    if rows.is_empty() && sommerfeld_outcome == SommerfeldOutcome::Declined {
        sommerfeld_outcome = if deck_has_plane_wave(deck) {
            SommerfeldOutcome::DeclinedReceive
        } else {
            SommerfeldOutcome::NotRequested
        };
    }

    Ok((rows, sommerfeld_outcome))
}

pub(super) fn build_source_rows(deck: &nec_model::deck::NecDeck) -> Vec<SourceRow> {
    deck.cards
        .iter()
        .filter_map(|card| {
            let Card::Ex(ex) = card else { return None };
            Some(SourceRow {
                excitation_type: ex.excitation_type,
                tag: ex.tag,
                seg: ex.segment,
                i4: ex.i4,
                voltage_real: ex.voltage_real,
                voltage_imag: ex.voltage_imag,
            })
        })
        .collect()
}

pub(super) fn build_load_rows(deck: &nec_model::deck::NecDeck) -> Vec<LoadRow> {
    deck.cards
        .iter()
        .filter_map(|card| {
            let Card::Ld(ld) = card else { return None };
            Some(LoadRow {
                load_type: ld.load_type,
                tag: ld.tag,
                seg_first: ld.seg_first,
                seg_last: ld.seg_last,
                f1: ld.f1,
                f2: ld.f2,
                f3: ld.f3,
            })
        })
        .collect()
}

/// The frequencies this deck will be solved at.
///
/// Delegates: the expansion lives in `nec_solver::frequency`, which the GUI, the
/// bindings and the validator also use. This function read only the **first**
/// `FR` card and treated an unrecognised `step_type` as "the start frequency
/// alone"; `nec2c` takes the **last** card and treats it as linear, so both were
/// wrong and differently wrong from `fnec_py` (FND-057).
pub(super) fn frequencies_from_fr(deck: &nec_model::deck::NecDeck) -> Vec<f64> {
    nec_solver::frequencies_hz(deck)
}

/// What a probe solves with, and the words its refusals and caveats use: each
/// frontend reaches the MPIE in its own way (FND-218, FND-271), and the
/// no-frequency refusal names a remedy the route can take (FND-070).
pub(crate) struct ProbeConfig<'a> {
    pub(crate) solver_mode: SolverMode,
    pub(crate) ground_solver: GroundSolver,
    pub(crate) execution_mode: ExecutionMode,
    /// Joined on a refusal of the geometry the MPIE would solve.
    pub(crate) refusal_mpie_remedy: &'a str,
    /// The geometry caveats' remedies, when the probe prints them.
    pub(crate) remedies: nec_solver::validate::Remedies<'a>,
    /// Appended to the refusal of a deck with no frequency.
    pub(crate) no_frequency_remedy: &'a str,
    /// Put before every stderr line the probe prints: empty for a route with one
    /// deck, `mesh 3x: ` for `fnec converge`, whose caveats are each mesh's own.
    pub(crate) stderr_prefix: &'a str,
}

/// Which of the deck's `FR` frequencies a probe solves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeFrequencies {
    /// Every frequency, in `FR` order (`fnec converge`).
    All,
    /// The first only: a resonance search's template carries one.
    First,
}

/// One solved frequency of a probe: its feedpoint rows, every source in the
/// report's order, and its `diag:` line (whose `route=` names the basis solved).
pub(crate) struct ProbePoint {
    pub(crate) freq_hz: f64,
    pub(crate) feedpoints: Vec<nec_report::FeedpointRow>,
    pub(crate) diag_line: String,
}

/// Solve a parsed deck for its feedpoints, as a helper route does: the geometry,
/// every refusal the main command makes (`pre_solve_error`, FND-173), the
/// excitation, the geometry caveats when `print_caveats` (once per deck, in the
/// caller's words — not once per probe, FND-218), then one
/// [`solve_frequency_point`] per frequency, each point's negative-resistance
/// sentences printed as its own. `sweep --resonance` and `fnec converge` run on
/// this; a route with its own copy of the sequence is how FND-173 happened.
pub(crate) fn probe_feedpoints(
    deck: &nec_model::deck::NecDeck,
    cfg: &ProbeConfig<'_>,
    which: ProbeFrequencies,
    print_caveats: bool,
) -> Result<Vec<ProbePoint>, String> {
    let segs = cfg
        .solver_mode
        .geometry(deck)
        .map_err(|e| e.with_mpie_remedy(cfg.refusal_mpie_remedy))?;
    let ground = nec_solver::ground_model_from_deck(deck);
    if let Some(err) = nec_solver::validate::pre_solve_error(deck, &segs, &ground) {
        return Err(err);
    }
    let v_vec = nec_solver::build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let wire_endpoints = nec_solver::wire_endpoints_from_segs(&segs);
    let per_wire_basis_feasible = wire_endpoints.iter().all(|&(first, last)| last > first);

    let mut freqs = frequencies_from_fr(deck);
    if freqs.is_empty() {
        return Err(
            nec_solver::validate::no_frequency_error(&freqs, cfg.no_frequency_remedy)
                .unwrap_or_else(|| "probe: no frequency".to_string()),
        );
    }
    if which == ProbeFrequencies::First {
        freqs.truncate(1);
    }

    // The Hallén caveats describe the Hallén geometry; the MPIE solves what they
    // warn of, as `solve_frequency_point` itself skips them there.
    if print_caveats && !matches!(cfg.solver_mode, SolverMode::Mpie) {
        let shown = match which {
            // A probe solves without Laplace loads (below), so its caveats see none.
            ProbeFrequencies::First => nec_solver::validate::hallen_geometry_caveats(
                deck,
                &segs,
                &[],
                &ground,
                freqs[0],
                false,
                cfg.remedies,
            ),
            ProbeFrequencies::All => nec_solver::validate::hallen_geometry_caveats_swept(
                deck,
                &segs,
                &[],
                &ground,
                &freqs,
                false,
                cfg.remedies,
            ),
        };
        for w in shown {
            eprintln!("{}warning: {w}", cfg.stderr_prefix);
        }
        for w in nec_solver::validate::solver_neutral_geometry_warnings(&segs) {
            eprintln!("{}warning: {w}", cfg.stderr_prefix);
        }
    }

    // `FNEC_HALLEN_REL_RESIDUAL_CAVEAT` or the default; refused here, before any
    // solve, as the main command refuses it.
    let threshold = nec_solver::validate::residual_caveat_threshold()?;
    let mut worst_rel: Option<f64> = None;
    let mut points = Vec::with_capacity(freqs.len());
    for freq_hz in freqs {
        let solved = solve_frequency_point(
            deck,
            &segs,
            &wire_endpoints,
            per_wire_basis_feasible,
            &v_vec,
            &ground,
            &[],
            cfg.solver_mode,
            PulseRhsMode::Nec2,
            cfg.execution_mode,
            SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT,
            freq_hz,
            cfg.ground_solver,
            &[], // Laplace loads apply to the main command's solve path only.
            GeometryCaveats::ShownByCaller,
        )?;
        // A probe is not a sweep point: each one still says so itself, as
        // before the sweep caveat was aggregated (FND-069).
        for w in &solved.negative_r {
            eprintln!("{}warning: {w}", cfg.stderr_prefix);
        }
        worst_rel = nec_solver::validate::worst_residual(worst_rel, solved.hallen_rel_res);
        let feedpoints = match solved.report {
            PointOutput::Tables(point) => point.rows.clone(),
            PointOutput::Text(_) => {
                return Err("probe: a local solve carries no tables".to_string());
            }
        };
        points.push(ProbePoint {
            freq_hz,
            feedpoints,
            diag_line: solved.diag_line,
        });
    }
    // FND-302 once per probe, at its worst point, as the main command does per run.
    if let Some(w) =
        worst_rel.and_then(|rel| nec_solver::validate::residual_caveat_at(rel, threshold))
    {
        eprintln!("{}warning: {w}", cfg.stderr_prefix);
    }
    Ok(points)
}

/// What the GPU-resident solve needs at one frequency point, built once the
/// point is known to be one the device takes.
struct GpuResidentPlan {
    hallen_rhs: nec_solver::HallenRhs,
    merged_endpoints: Vec<(usize, usize)>,
    junctions: Vec<(usize, usize, f64)>,
}

/// Whether the GPU-resident Hallén fill+solve (PH7-CHK-003) takes this point,
/// decided once and before the matrix is filled, so the fill and the solve
/// cannot disagree about it (FND-293): `--exec gpu`, the Hallén basis,
/// [`nec_solver::gpu_resident_decline`] (class, stamps, Laplace loads), at least
/// `nec_accel::MIN_GPU_RESIDENT_SEGS` segments, and an excitation outside the
/// homogeneous span (FND-227). Every condition but the adapter: a device that is
/// missing or fails declines at run time, and says so on stderr.
fn gpu_resident_plan(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    laplace_loads: &[nec_solver::LaplaceLoad],
    solver_mode: SolverMode,
    execution_mode: ExecutionMode,
    freq_hz: f64,
) -> Result<Option<GpuResidentPlan>, String> {
    if execution_mode != ExecutionMode::Gpu
        || !matches!(solver_mode, SolverMode::Hallen)
        || segs.len() < nec_accel::MIN_GPU_RESIDENT_SEGS
        || nec_solver::gpu_resident_decline(deck, segs, ground, &[freq_hz], laplace_loads).is_some()
    {
        return Ok(None);
    }
    let hallen_rhs = build_hallen_rhs(deck, segs, freq_hz).map_err(|e| e.to_string())?;
    let (merged_endpoints, junctions) = nec_solver::merged_grouping(segs);
    // An excitation the homogeneous constants absorb whole (FND-227): the CPU
    // solve refuses it and says why.
    if nec_solver::merged_excitation_in_homogeneous_span(
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        &hallen_rhs.sin_vec,
        &merged_endpoints,
        &junctions,
    ) {
        return Ok(None);
    }
    Ok(Some(GpuResidentPlan {
        hallen_rhs,
        merged_endpoints,
        junctions,
    }))
}

/// Run the GPU-resident Hallén fill+solve on a point [`gpu_resident_plan`]
/// accepted. Returns `None` (the caller uses the routed CPU solve) when no wgpu
/// adapter is available or the device declines; each decline has said why.
fn maybe_gpu_resident_hallen(
    segs: &[Segment],
    plan: &GpuResidentPlan,
    freq_hz: f64,
) -> Option<nec_solver::HallenSolution> {
    let GpuResidentPlan {
        hallen_rhs,
        merged_endpoints: wire_endpoints,
        junctions,
    } = plan;
    let z_inputs: Vec<nec_accel::ZSegmentInput> = segs
        .iter()
        .map(|s| nec_accel::ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();

    // Said here, where the path is actually taken, and only below the crossover.
    crate::warnings::warn_gpu_resident_solve_is_slower(segs.len());

    let x = pollster::block_on(nec_accel::solve_hallen_gpu_resident(
        &z_inputs,
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        &hallen_rhs.sin_vec,
        wire_endpoints,
        &nec_solver::sin_eligible(wire_endpoints, junctions),
        &nec_solver::hallen_constraint_rows(
            wire_endpoints,
            junctions,
            &segs.iter().map(|s| s.length).collect::<Vec<_>>(),
        ),
        freq_hz,
    ))
    // Every decline has already said why on stderr; the caller falls back.
    .ok()?;

    let n = segs.len();
    if x.len() < n {
        return None;
    }
    let currents = x[..n].to_vec();
    let c_hom_per_wire = x[n..].to_vec();
    let c_hom = c_hom_per_wire
        .first()
        .copied()
        .unwrap_or(Complex64::new(0.0, 0.0));
    Some(nec_solver::HallenSolution {
        currents,
        c_hom_per_wire,
        c_hom,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn solve_frequency_point(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    wire_endpoints: &[(usize, usize)],
    per_wire_basis_feasible: bool,
    v_vec: &[Complex64],
    ground: &GroundModel,
    pattern_points: &[FarFieldPoint],
    solver_mode: SolverMode,
    pulse_rhs_mode: PulseRhsMode,
    execution_mode: ExecutionMode,
    sin_fallback_rel_max: f64,
    freq_hz: f64,
    ground_solver: GroundSolver,
    laplace_loads: &[nec_solver::LaplaceLoad],
    geometry_caveats: GeometryCaveats,
) -> Result<FrequencySolveResult, String> {
    // Incident plane waves and current sources are solved on the Hallén path
    // only (crate::planewave / solve_hallen_current_source).
    if deck_has_plane_wave(deck) && !matches!(solver_mode, SolverMode::Hallen) {
        return Err("EX: incident plane-wave excitation requires --solver hallen".to_string());
    }
    if deck_has_current_source(deck) && !matches!(solver_mode, SolverMode::Hallen) {
        return Err("EX: current-source excitation requires --solver hallen".to_string());
    }
    if matches!(solver_mode, SolverMode::Mpie) {
        // Everything the deck can carry that the MPIE cannot solve is refused by
        // `solve_mpie_session` itself, so no caller can bypass it; loads, deck and
        // `--loads-config` alike, it stamps (FND-222 lifted).
        if let Some(u) = nec_solver::mpie_unsupported(deck) {
            return Err(format!("{}: not supported with --solver mpie", u.subject()));
        }
    }

    let mut v_vec_pulse = scale_pulse_rhs(v_vec, pulse_rhs_mode, freq_hz);

    // The sinusoidal basis holds a conductor as one index range, so it solves the
    // segments in chain order and restores the currents to deck order: a collinear
    // chain was solved or refused by the order of its GW cards (FND-233). The whole
    // arm runs on `sin_segs` — assembly, load columns, route and source — because
    // each of them reads the chain off the array; permuting a finished matrix would
    // leave the load columns shaped over the unmerged wires.
    let sin_order = if matches!(solver_mode, SolverMode::Sinusoidal) {
        nec_solver::SegmentOrder::chains(segs)
    } else {
        nec_solver::SegmentOrder::default()
    };
    let sin_segs = sin_order.segments(segs);

    // Whether the device solves this point, decided before the fill: the device
    // fills the matrix (in f32) only for a point it then solves itself (FND-293).
    let resident_plan = gpu_resident_plan(
        deck,
        segs,
        ground,
        laplace_loads,
        solver_mode,
        execution_mode,
        freq_hz,
    )?;
    // Whether the matrix the solve used was filled on the device, for the record.
    let mut filled_on_device = false;
    let mut z_mat = match solver_mode {
        SolverMode::Hallen => {
            // The device fills the Z-matrix in f32 only for a point the resident
            // solve takes (free space, nothing stamped). It used to fill every
            // `--exec gpu` free-space deck of 128 segments or more, and when the
            // resident solve then declined — a load, a bend — the CPU solved the
            // f32 matrix: 106.383566 + j36.388674 against the f64 106.383595 +
            // j36.388745 on a 201-segment deck with `LD 4`, recorded as
            // `solve_exec=cpu`, and different from the worker, which always fills
            // in f64 (FND-293). A declined point is now filled on the CPU in f64.
            //
            // A minimum segment count guards against wgpu device-init overhead
            // dominating for small problems where CPU assembly is < 1 ms. It is
            // above `nec_accel::MIN_GPU_RESIDENT_SEGS`, so a resident solve of
            // fewer segments reads `fill_exec=cpu solve_exec=gpu`: the device
            // re-fills from raw segment inputs for its own solve, and this host
            // matrix serves the residual check only.
            const MIN_GPU_ZMATRIX_SEGS: usize = 128;
            let try_gpu = resident_plan.is_some() && segs.len() >= MIN_GPU_ZMATRIX_SEGS;
            if try_gpu {
                let z_inputs: Vec<nec_accel::ZSegmentInput> = segs
                    .iter()
                    .map(|s| nec_accel::ZSegmentInput {
                        midpoint: s.midpoint,
                        direction: s.direction,
                        length: s.length,
                        radius: s.radius,
                    })
                    .collect();
                match pollster::block_on(nec_accel::fill_zmatrix_wgpu(&z_inputs, freq_hz)) {
                    Ok(elems) => {
                        let n = segs.len();
                        let flat: Vec<Complex64> = elems
                            .iter()
                            .map(|e| Complex64::new(e.re as f64, e.im as f64))
                            .collect();
                        let mut z = ZMatrix::from_flat(n, flat);
                        z.set_segments(segs);
                        filled_on_device = true;
                        z
                    }
                    Err(why) => {
                        eprintln!("warning: --exec gpu: {why}; falling back to CPU Z-matrix fill");
                        assemble_z_matrix_with_ground(segs, freq_hz, ground)
                    }
                }
            } else {
                assemble_z_matrix_with_ground(segs, freq_hz, ground)
            }
        }
        SolverMode::Pulse | SolverMode::Continuity => assemble_pocklington_matrix(segs, freq_hz),
        SolverMode::Sinusoidal => {
            // Sinusoidal mode uses the Hallén thin-wire Z-matrix (same as Hallen),
            // not the Pocklington EFIE matrix. The accurate basis only matters for
            // the solve step, not the matrix assembly.
            assemble_z_matrix_with_ground(&sin_segs, freq_hz, ground)
        }
        // The MPIE assembles and solves its own (triangle-basis) system in the
        // i_vec step below; this Hallén Z-matrix is unused by it. Assembling it
        // keeps the downstream stamps and diagnostics well-formed (MPIE decks with
        // TL/NT are rejected before this point; loads it stamps itself).
        SolverMode::Mpie => assemble_z_matrix_with_ground(segs, freq_hz, ground),
    };

    // LD loads, TL lines and NT networks, built once through the shared seam so
    // every frontend applies the same set (FND-015). Laplace loads are composed
    // into the same diagonal: they are a CLI-only input (`--loads-config`), so they
    // are not part of the deck's own stamps.
    let mut stamps = nec_solver::build_deck_stamps(deck, segs, freq_hz);
    // For the decision record: whether the DECK's own cards load the structure,
    // read from the stamps before any Laplace load is composed in.
    let deck_loaded = stamps.diagonal.iter().any(|z| z.norm() > 0.0);
    // The run's diagonal through the one function that answers "which loads"
    // (FND-288), so the caveats and the MPIE see what this solve stamps.
    let (diagonal, laplace_warnings) =
        nec_solver::run_load_diagonal(deck, segs, freq_hz, laplace_loads);
    stamps.diagonal = diagonal;
    for warning in &stamps.warnings {
        eprintln!("warning: {warning}");
    }
    // A Laplace load that cannot be evaluated is refused, like an LD card fnec
    // cannot apply (FND-161): skipping it would solve without the load.
    if let Some(problem) = laplace_warnings.first() {
        return Err(format!("--loads-config: {problem}"));
    }
    // What the negative-resistance caveat may say about loads is a property of
    // this diagonal, not of the deck's cards (FND-209).
    let run_loads = nec_solver::validate::RunLoads::of(&stamps.diagonal);
    // Loads enter in the form the basis that runs derives for them (FND-122,
    // FND-124). Hallen: as matrix columns, stamped by the session once the route is
    // known. Sinusoidal: the same columns, since it solves the Hallen matrix in a
    // projected basis. Pulse and continuity: a diagonal of Z_p/Δl, scaled exactly
    // as the source vector is, because a load IS a source of −Z_p·I_p.
    match solver_mode {
        SolverMode::Hallen | SolverMode::Mpie => {}
        SolverMode::Sinusoidal => {
            nec_solver::stamp_hallen_load_columns(
                &mut z_mat,
                &sin_segs,
                freq_hz,
                &sin_order.apply(&stamps.diagonal),
                None,
            );
        }
        SolverMode::Pulse | SolverMode::Continuity => {
            let diagonal = nec_solver::pocklington_load_diagonal(segs, &stamps.diagonal);
            z_mat.add_to_diagonal(&scale_pulse_rhs(&diagonal, pulse_rhs_mode, freq_hz));
        }
    }
    // TL/NT networks are solved by superposition inside the Hallén session
    // (FND-123); the other bases have no such solve yet, and the old series-Z
    // stamp they used to get was inert. Refused rather than answered.
    // A wire touching PEC ground is solved by explicit images inside the Hallén
    // session (FND-082); the other bases would put a free-end row at the base and
    // answer billions of ohms. MPIE refuses ground contact itself.
    if matches!(
        solver_mode,
        SolverMode::Sinusoidal | SolverMode::Pulse | SolverMode::Continuity
    ) && matches!(ground, GroundModel::PerfectConductor)
        && nec_solver::ground_contact::touches_ground(segs)
    {
        return Err(
            "wires touching the ground are supported on --solver hallen only (FND-082)".to_string(),
        );
    }
    // The sinusoidal basis is the plain merged-conductor solve: it has no
    // conductor-path basis for a bend or split (FND-121) and no section graph for
    // a junction or loop (FND-162). It answered those decks anyway, silently — a
    // split-V 10.27 - j731.22 against Hallén's 270.43 + j443.20 and nec2c's
    // 268.56 + j452.26, a T 4.19 - j1010.07 against nec2c's 107.54 - j366.35 —
    // and its residual check could not see it (FND-203). The routing decision is
    // the Hallén session's own, so the two cannot disagree about what a deck is.
    if matches!(solver_mode, SolverMode::Sinusoidal) {
        let route = nec_solver::hallen_route(deck, &sin_segs);
        if route.paths || route.unsupported_topology {
            return Err(
                "--solver sinusoidal solves straight wires and collinear chains only; this \
                 deck has a bend, split, junction or loop, which needs the conductor-path or \
                 section-graph basis of --solver hallen (FND-203)"
                    .to_string(),
            );
        }
    }
    if stamps.has_networks && !matches!(solver_mode, SolverMode::Hallen | SolverMode::Mpie) {
        return Err(format!(
            "TL/NT networks are supported on --solver hallen only (FND-123); \
             --solver {} has no network solve",
            match solver_mode {
                SolverMode::Pulse => "pulse",
                SolverMode::Continuity => "continuity",
                _ => "sinusoidal",
            }
        ));
    }
    let mut pulse_current_sources = if matches!(solver_mode, SolverMode::Pulse) {
        collect_pulse_current_source_constraints(deck, segs)?
    } else {
        Vec::new()
    };
    if matches!(solver_mode, SolverMode::Pulse) {
        apply_pulse_current_source_constraints(
            &mut z_mat,
            &mut v_vec_pulse,
            &mut pulse_current_sources,
        );
    }
    let diag_spread = matrix_diagonal_spread(&z_mat);
    // `None` unless the sinusoidal basis ran: its pre-fallback residual. A 0 for
    // every other route read as a perfect sinusoidal solve that never happened, the
    // FND-306 class, so it prints `n/a` too (D8).
    let mut sin_rel_res: Option<f64> = None;
    // Set by the current-source path: the solved port voltage V (feedpoint Z=V/i0).
    let mut current_source_port: Option<Complex64> = None;

    // The current each source delivers where it differs from its wire current: a
    // driven segment that is also a TL/NT port feeds the network in parallel.
    let mut network_branch: Vec<(usize, Complex64)> = Vec::new();
    // Whether the device produced this point's currents. `--exec gpu` alone does
    // not say: the label read `gpu(cpu-fallback)` on every point, including the
    // ones the device solved.
    let mut ran_on_gpu = false;
    let (i_vec, diag_res, diag_label) = match solver_mode {
        SolverMode::Hallen => {
            // One decision, shared with the GUI, the bindings and the worker
            // (FND-121). It used to be four arms here and nowhere else, so those
            // three answered a bent or split geometry on the plain basis while
            // this frontend used conductor paths: 264.88 + j410.86 here against
            // 9.15 - j767.60 from the worker on the same deck, both silent.
            //
            // The GPU-resident fill+solve implements exactly one class — the
            // plain delta-gap route in free space — so it asks the shared
            // predicate rather than assuming, and declines anything else —
            // decided once, in `resident_plan`, before the fill.
            let gpu_sol = resident_plan
                .as_ref()
                .and_then(|plan| Some((maybe_gpu_resident_hallen(segs, plan, freq_hz)?, plan)));

            if let Some((sol, plan)) = gpu_sol {
                let GpuResidentPlan {
                    hallen_rhs,
                    merged_endpoints,
                    junctions: junction_tuples,
                } = plan;
                ran_on_gpu = true;
                // This arm exists because it bypasses `solve_hallen_routed`, so
                // the guard there does not reach it (FND-126).
                nec_solver::check_currents_finite(&sol.currents).map_err(|e| e.to_string())?;
                let h = nec_solver::hallen_homogeneous(
                    &hallen_rhs.cos_vec,
                    &hallen_rhs.sin_vec,
                    &sol.c_hom_per_wire,
                    merged_endpoints,
                    junction_tuples,
                );
                // Against `z_mat`, which at 128 segments or more is the device's
                // own f32 fill: the check confirms the solve of that system, not
                // of the f64 problem (FND-307, open).
                let res = nec_solver::ResidualInputs::plain(
                    h,
                    hallen_rhs.rhs.clone(),
                    merged_endpoints.clone(),
                )
                .residual(&z_mat, &sol.currents);
                (
                    sol.currents,
                    Some((res, nec_solver::validate::ResidualKind::DeviceF32)),
                    "hallen",
                )
            } else {
                let routed = nec_solver::solve_hallen_routed(
                    deck,
                    segs,
                    &mut z_mat,
                    freq_hz,
                    &stamps.diagonal,
                )
                .map_err(|e| e.to_string())?;
                current_source_port = routed.port_voltage;
                network_branch.clone_from(&routed.network_branch);
                // Computed by the session against the system it solved, corner term
                // included (FND-302); `None` where the route computes none, which
                // the diag line prints as `n/a`, not as a perfect 0 (FND-306).
                (
                    routed.currents,
                    routed
                        .residual
                        .map(|r| (r, nec_solver::validate::ResidualKind::HallenF64)),
                    routed.route.mode_label(),
                )
            }
        }
        SolverMode::Pulse => {
            let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
            let res = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
            (
                i,
                Some((res, nec_solver::validate::ResidualKind::Pulse)),
                "pulse",
            )
        }
        SolverMode::Continuity => {
            if !per_wire_basis_feasible {
                eprintln!(
                    "warning: continuity solver requires >=2 segments per wire; falling back to pulse"
                );
                let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                let res = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                (
                    i,
                    Some((res, nec_solver::validate::ResidualKind::Pulse)),
                    "continuity->pulse",
                )
            } else {
                let i = solve_with_continuity_basis_per_wire(&z_mat, &v_vec_pulse, wire_endpoints)
                    .map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                if r <= CONTINUITY_REL_RESIDUAL_MAX {
                    (
                        i,
                        Some(((a, r), nec_solver::validate::ResidualKind::Pulse)),
                        "continuity",
                    )
                } else {
                    eprintln!(
                        "warning: continuity residual {:.3e} > {:.3e}; falling back to pulse",
                        r, CONTINUITY_REL_RESIDUAL_MAX
                    );
                    let i2 = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                    let res2 = residual_zi_minus_v(&z_mat, &i2, &v_vec_pulse);
                    (
                        i2,
                        Some((res2, nec_solver::validate::ResidualKind::Pulse)),
                        "continuity->pulse(residual)",
                    )
                }
            }
        }
        SolverMode::Sinusoidal => {
            let (i, res, label) = {
                // NEC2-style sinusoidal Galerkin basis on the Hallén integral equation.
                // Uses the Hallén thin-wire Z-matrix (assembled above) with piecewise-
                // sinusoidal expansion functions and pulse testing (projection).
                // Each source signed for the flipped frame (FND-249); the deck's own
                // `V` still drives the feedpoint readout, against restored currents.
                let sources: Vec<(usize, Complex64)> =
                    nec_solver::delta_gap_sources(deck, &sin_segs)
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(|(i, v)| (i, sin_order.flips_source(i, v)))
                        .collect();
                let hallen_rhs =
                    nec_solver::build_hallen_rhs_from_sources(&sin_segs, freq_hz, &sources);
                // The Hallén arm's grouping, not the raw wires: it decides which
                // conductors take the sin homogeneous term, and a collinear split
                // must be one conductor on every `--solver` (FND-158).
                let (sin_endpoints, junction_tuples) = nec_solver::merged_grouping(&sin_segs);
                let wire_endpoints = &sin_endpoints[..];
                // The basis needs two segments on every conductor it solves, and
                // the conductors are this merged grouping, not the deck's wires: a
                // one-segment wire inside a collinear chain is part of a longer
                // conductor. Judged per wire, that deck fell back to the
                // experimental pulse basis and answered -75.957 + j1.406 where
                // nec2c reads 78.806 + j40.991 (FND-273). Refinement, FND-203's
                // routing refusal and the ground checks leave no deck with a
                // one-segment conductor here; if one ever arrives it is a bug,
                // not the user's deck.
                if wire_endpoints.iter().any(|&(first, last)| last <= first) {
                    return Err("internal: a one-segment conductor reached the sinusoidal \
                                basis (FND-273)"
                        .to_string());
                }
                let sol = solve_hallen_sinusoidal_basis(
                    &z_mat,
                    &hallen_rhs.rhs,
                    &hallen_rhs.cos_vec,
                    &hallen_rhs.sin_vec,
                    wire_endpoints,
                    &junction_tuples,
                )
                .map_err(|e| e.to_string())?;
                let h = nec_solver::hallen_homogeneous(
                    &hallen_rhs.cos_vec,
                    &hallen_rhs.sin_vec,
                    &sol.c_hom_per_wire,
                    wire_endpoints,
                    &junction_tuples,
                );
                let (a, r) = nec_solver::ResidualInputs::plain(
                    h,
                    hallen_rhs.rhs.clone(),
                    sin_endpoints.clone(),
                )
                .residual(&z_mat, &sol.currents);
                sin_rel_res = Some(r);
                if r <= sin_fallback_rel_max {
                    (
                        sol.currents,
                        Some((
                            (a, r),
                            nec_solver::validate::ResidualKind::SinusoidalProjection,
                        )),
                        "sinusoidal",
                    )
                } else {
                    eprintln!(
                        "warning: sinusoidal residual {:.3e} > {:.3e}; falling back to hallen",
                        r, sin_fallback_rel_max
                    );
                    let hallen_sol = solve_hallen(
                        &z_mat,
                        &hallen_rhs.rhs,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.sin_vec,
                        wire_endpoints,
                        &junction_tuples,
                    )
                    .map_err(|e| e.to_string())?;
                    let h2 = nec_solver::hallen_homogeneous(
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.sin_vec,
                        &hallen_sol.c_hom_per_wire,
                        wire_endpoints,
                        &junction_tuples,
                    );
                    let res2 = nec_solver::ResidualInputs::plain(
                        h2,
                        hallen_rhs.rhs.clone(),
                        sin_endpoints.clone(),
                    )
                    .residual(&z_mat, &hallen_sol.currents);
                    (
                        hallen_sol.currents,
                        Some((res2, nec_solver::validate::ResidualKind::HallenF64)),
                        "sinusoidal->hallen(residual)",
                    )
                }
            };
            (sin_order.restore_currents(&i), res, label)
        }
        SolverMode::Mpie => {
            // One mapping for every variant: the special-cased `NoVoltageSource`
            // arm existed only to preserve a byte-identical string, and it named
            // "EX type 0" where the shared message now correctly says type 0 or 5.
            let currents =
                nec_solver::solve_mpie_session(deck, segs, ground, freq_hz, laplace_loads)
                    .map_err(|e| format!("--solver mpie: {e}"))?;
            // The MPIE computes no residual: "n/a", not a perfect 0 (FND-306).
            (currents, None, "mpie")
        }
    };

    let mut feed_i_vec = i_vec.clone();
    for (seg, branch) in &network_branch {
        feed_i_vec[*seg] += branch;
    }
    let (rows, sommerfeld_outcome) = build_feedpoint_rows(
        deck,
        segs,
        v_vec,
        &feed_i_vec,
        &pulse_current_sources,
        solver_mode,
        current_source_port,
        freq_hz,
        ground,
        ground_solver,
    )?;

    // PH9-CHK-005: guard the junction-fed feedpoint limitation. When the driven
    // segment sits at a wire junction the feed current splits across the joined
    // wires, so the single-segment V/I is not the true feedpoint impedance and can
    // be unphysical (e.g. negative resistance). Warn rather than report it as
    // trustworthy; accurate junction-fed impedance is PH9-CHK-002.
    // These warn about limitations of the Hallén + scalar-Γ paths — unsupported
    // topologies, junction-fed V/I inaccuracy, and the reflection-coefficient
    // near-ground impedance. The MPIE solves all three correctly (junctions/loops,
    // and the Sommerfeld surface wave in the Z-matrix), so skip them there.
    if !matches!(solver_mode, SolverMode::Mpie) {
        // Through the shared producer (FND-020), so a caveat added there reaches
        // the distributed path too instead of only this one. A request that
        // actually applied DID model the surface wave, which changes what the
        // low-ground caveat may claim.
        let shown = match geometry_caveats {
            // The run's loads, `--loads-config` included: the caveat about a load
            // beside a bend must see the load the solve stamps (FND-288).
            GeometryCaveats::PerPoint => nec_solver::validate::hallen_geometry_caveats(
                deck,
                segs,
                laplace_loads,
                ground,
                freq_hz,
                matches!(sommerfeld_outcome, SommerfeldOutcome::Applied),
                nec_solver::validate::Remedies {
                    mpie: Some(CLI_MPIE_REMEDY),
                    mpie_low_ground: Some(CLI_MPIE_REMEDY),
                    // Only to a run that has not asked: one that asked and was declined
                    // already has the decline's own warning, and telling it to re-run
                    // with the flag it passed would be advice it has taken (FND-217).
                    sommerfeld: (ground_solver == GroundSolver::Rcm)
                        .then_some(CLI_SOMMERFELD_REMEDY),
                },
            ),
            GeometryCaveats::ShownByCaller => Vec::new(),
        };
        for w in shown {
            eprintln!("warning: {w}");
        }
        // Stays here: only this frontend can make the request that gets declined.
        warn_if_sommerfeld_declined(sommerfeld_outcome);
    }
    let negative_r = negative_resistance_warnings(&rows, deck, segs, solver_mode, run_loads);
    let min_feed_re = rows.iter().map(|r| r.z_in.re).reduce(f64::min);

    // One row per deck segment: a refined end's flanks are not the deck's, and its
    // centre's current is the current at the deck segment's midpoint (FND-227).
    // PH9-CHK-004 / FND-299: the PT card selects the printed segments.
    let printed = PtSelection::of(deck);
    let current_table: Vec<CurrentRow> = segs
        .iter()
        .enumerate()
        .filter(|(_, seg)| seg.is_deck_segment() && printed.prints(seg))
        .map(|(idx, seg)| CurrentRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            current: i_vec[idx],
        })
        .collect();

    let source_table = build_source_rows(deck);
    let load_table = build_load_rows(deck);

    // The device's far-field kernel sums the real segments in free space: it has
    // no image and no reflected field. It ran on every `--exec gpu` deck, so over
    // GN 1 or GN 2 the pattern was the free-space one — a vertical dipole over
    // PEC −8.62 / −2.56 / +0.67 dBi at θ 20/40/60 against the CPU's −3.13 /
    // −0.26 / −15.04 (nec2c −3.13 / −0.27 / −15.04) — while the solve had
    // correctly declined the ground
    // deck to the CPU (FND-205). The deck class keys the solve; it keys the
    // pattern too.
    let gpu_pattern =
        execution_mode == ExecutionMode::Gpu && matches!(ground, GroundModel::FreeSpace);
    if execution_mode == ExecutionMode::Gpu && !gpu_pattern && !pattern_points.is_empty() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            eprintln!(
                "info: --exec gpu: the radiation pattern over ground runs on the CPU \
                 (the device's far-field kernel is free-space only)"
            );
        });
    }
    // Whether the device actually computed the pattern, for the decision record:
    // the exec label alone said `gpu` while the pattern ran elsewhere (FND-205).
    let mut rp_on_device = false;
    let mut pattern_table: Vec<PatternRow> = if pattern_points.is_empty() {
        Vec::new()
    } else if gpu_pattern {
        // Attempt wgpu RP kernel dispatch (gate G4).
        // Compute total radiated power on CPU for gain normalisation — the GPU
        // computes radiation intensity components, not normalised gain. Free
        // space only (above), so the power integral is the full sphere.
        let total_radiated = integrate_radiated_power(segs, &i_vec, freq_hz, false);
        let k = 2.0 * std::f64::consts::PI * freq_hz / C0;

        let gpu_segments: Vec<_> = segs
            .iter()
            .map(|seg| nec_accel::kernel_reference::GpuSegment {
                midpoint: seg.midpoint,
                direction: seg.direction,
                length: seg.length,
            })
            .collect();

        let points_tuples: Vec<(f64, f64)> = pattern_points
            .iter()
            .map(|p| (p.theta_deg, p.phi_deg))
            .collect();

        let gpu_results = pollster::block_on(nec_accel::wgpu_device::run_rp_farfield_batch_wgpu(
            &gpu_segments,
            &i_vec,
            k,
            total_radiated,
            &points_tuples,
        ));

        match gpu_results {
            Some(rows) => {
                rp_on_device = true;
                rows.iter()
                    .map(|r| PatternRow {
                        theta_deg: r.theta_deg,
                        phi_deg: r.phi_deg,
                        gain_total_dbi: r.gain_total_dbi,
                        gain_theta_dbi: r.gain_theta_dbi,
                        gain_phi_dbi: r.gain_phi_dbi,
                        axial_ratio: r.axial_ratio,
                        // The device path computes gains only.
                        e_field: None,
                        ellipse: None,
                    })
                    .collect()
            }
            None => {
                // No adapter available — fall back to CPU path silently.
                eprintln!("warning: --exec gpu: no wgpu adapter available, falling back to CPU RP");
                let results =
                    compute_radiation_pattern(segs, &i_vec, freq_hz, pattern_points, ground);
                results.iter().map(pattern_row).collect()
            }
        }
    } else {
        // Standard CPU path
        let results = compute_radiation_pattern(segs, &i_vec, freq_hz, pattern_points, ground);
        results.iter().map(pattern_row).collect()
    };

    // PH9-CHK-003: the pattern gain is the directivity reduced by the radiation
    // efficiency η = P_radiated / P_input. compute_radiation_pattern returns
    // directivity; convert to gain here so the reported dBi matches nec2c's gain.
    // Over every ground: this was gated on a lossy finite ground, so a lossy load
    // in free space or over PEC printed its directivity as gain (FND-200).
    // The shared producer, not a second inline sum. It computed a different
    // number for a current source than this loop did, and since only the GUI
    // called it, the divergence was invisible from here (FND-114). Identical
    // arithmetic over an identical feedpoint set: `build_feedpoint_rows` has
    // already run and aborts the session on any feedpoint it cannot price, so
    // no feedpoint can reach this that the rows skipped.
    let p_in: f64 =
        nec_solver::feedpoint_input_power(deck, segs, v_vec, &feed_i_vec, current_source_port);
    // NEC-2's power budget (PAR-011): the loads' dissipation from the wire
    // currents and the diagonal the solve stamped; a network's loss is not
    // separated here, so a deck with one has none.
    let power_budget = (!rows.is_empty()).then(|| nec_report::PowerBudget {
        input_w: p_in,
        structure_loss_w: nec_solver::structure_loss_w(&i_vec, &stamps.diagonal),
        network_loss_w: (!stamps.has_networks).then_some(0.0),
    });
    if !pattern_table.is_empty() {
        // Through the shared producer, so the GUI's pattern view applies the same
        // correction rather than reporting directivity as gain (FND-053).
        // An RP card whose XNDA D digit asks for directive gain keeps its rows'
        // directivity, as nec2c prints "DIRECTIVE GAINS" for it (FND-298). The
        // table holds every card's grid in deck order, as the average does below.
        if let Some(delta_db) = nec_solver::gain_correction_db(segs, &i_vec, freq_hz, ground, p_in)
        {
            let mut offset = 0;
            for rp in deck.cards.iter().filter_map(|c| match c {
                Card::Rp(rp) => Some(rp),
                _ => None,
            }) {
                let end = (offset + (rp.n_theta * rp.n_phi) as usize).min(pattern_table.len());
                if !rp.directive_gain {
                    for row in &mut pattern_table[offset..end] {
                        for g in [
                            &mut row.gain_total_dbi,
                            &mut row.gain_theta_dbi,
                            &mut row.gain_phi_dbi,
                        ] {
                            if *g > -900.0 {
                                *g += delta_db;
                            }
                        }
                    }
                }
                offset = end;
            }
        }
    }

    // PH9-CHK-001: incident-plane-wave receive-pattern sweep (NTHETA·NPHI > 1).
    let receive_pattern_table = if deck_has_plane_wave(deck) {
        plane_wave_receive_sweep(deck, segs, &z_mat, freq_hz, ground, &stamps.diagonal)?
    } else {
        Vec::new()
    };

    // PH9-CHK-004: near electric field on the NE-card grid(s), magnetic on NH.
    let near_field_table = build_near_field_rows(deck, segs, &i_vec, freq_hz, ground);
    let near_h_field_table = build_near_h_field_rows(deck, segs, &i_vec, freq_hz, ground);

    // PH9-CHK-004: average power gain (RP XNDA A-digit), by NEC-2's rule
    // (FND-262): each RP card's grid is averaged on its own rows (the table holds
    // every card's grid in deck order), and the last card asking for it is the
    // one reported.
    let avg_power_gain = {
        let mut offset = 0;
        let mut avg = None;
        for rp in deck.cards.iter().filter_map(|c| match c {
            Card::Rp(rp) => Some(rp),
            _ => None,
        }) {
            let n = (rp.n_theta * rp.n_phi) as usize;
            if rp.avg_power_gain && offset + n <= pattern_table.len() {
                let gains: Vec<f64> = pattern_table[offset..offset + n]
                    .iter()
                    .map(|r| r.gain_total_dbi)
                    .collect();
                avg = nec_solver::average_power_gain(
                    &gains,
                    rp.n_theta as usize,
                    rp.n_phi as usize,
                    rp.theta0,
                    rp.d_theta,
                    rp.d_phi,
                );
            }
            offset += n;
        }
        avg
    };

    let pattern_rows = pattern_table.len();
    let near_field_points = near_field_table.len() + near_h_field_table.len();
    let sweep_summary = rows.first().map(|row| SweepPointSummary {
        freq_mhz: freq_hz / 1e6,
        tag: row.tag,
        seg: row.seg,
        z_re: row.z_in.re,
        z_im: row.z_in.im,
        caveat: matches!(solver_mode, SolverMode::Pulse | SolverMode::Continuity)
            .then_some(crate::cli_args::UNVALIDATED_SOLVER_CAVEAT),
        feedpoints: rows
            .iter()
            .map(|r| SweepFeedpoint {
                tag: r.tag,
                seg: r.seg,
                z_re: r.z_in.re,
                z_im: r.z_in.im,
            })
            .collect(),
    });
    let report = PointOutput::Tables(Box::new(PointReport {
        solver_mode: diag_label.to_string(),
        pulse_rhs: pulse_rhs_mode.as_contract_str().to_string(),
        caveat: matches!(solver_mode, SolverMode::Pulse | SolverMode::Continuity)
            .then(|| crate::cli_args::UNVALIDATED_SOLVER_CAVEAT.to_string()),
        frequency_hz: freq_hz,
        rows,
        source_table,
        load_table,
        current_table,
        pattern_table,
        receive_pattern_table,
        near_field_table,
        near_h_field_table,
        normalize_pattern: deck
            .cards
            .iter()
            .any(|c| matches!(c, Card::Rp(rp) if rp.normalize)),
        avg_power_gain,
        power_budget,
    }));
    let diag_line = format!(
        "diag: mode={diag_label} pulse_rhs={:?} exec={} freq_mhz={:.6} abs_res={} rel_res={} diag_spread={:.6e} sin_rel_res={} sin_fallback_rel_max={:.6e} {}",
        pulse_rhs_mode,
        exec_label(execution_mode, ran_on_gpu),
        freq_hz / 1e6,
        residual_field(diag_res.map(|((a, _), _)| a)),
        residual_field(diag_res.map(|((_, r), _)| r)),
        diag_spread,
        residual_field(sin_rel_res),
        sin_fallback_rel_max,
        decision_record(&DecisionInputs {
            deck,
            segs,
            solver_mode,
            ground,
            ground_solver,
            sommerfeld_outcome,
            deck_loaded,
            config_loaded: !laplace_loads.is_empty(),
            filled_on_device,
            solved_on_device: ran_on_gpu,
            pattern_rows,
            pattern_on_device: rp_on_device,
            near_field_points,
        }),
    );

    let bench = BenchRecord {
        mode: diag_label.to_string(),
        pulse_rhs: pulse_rhs_mode.as_contract_str().to_string(),
        exec: exec_label(execution_mode, ran_on_gpu).to_string(),
        freq_mhz: freq_hz / 1e6,
        abs_res: diag_res.map(|((a, _), _)| a),
        rel_res: diag_res.map(|((_, r), _)| r),
        diag_spread: Some(diag_spread),
        sin_rel_res,
    };

    // Judged or not is the residual's kind, set where it was computed: a device
    // (f32) solve, the sinusoidal projection and the pulse bases are not judged
    // by the FND-302 caveat (`ResidualKind::judged_by_hallen_caveat`).
    let hallen_rel_res = diag_res
        .filter(|(_, kind)| kind.judged_by_hallen_caveat())
        .map(|((_, r), _)| r);
    Ok(FrequencySolveResult {
        hallen_rel_res,
        report,
        diag_line,
        bench,
        sweep_summary,
        negative_r,
        run_loads,
        min_feed_re,
        ran_on_gpu,
        surface_wave_applied: sommerfeld_outcome == SommerfeldOutcome::Applied,
    })
}

/// What one run decided, for the parity sweep (`docs/dev/parity-sweep-design.md`).
///
/// The sweep compares two runs that must agree, and classifies a cell by what each
/// run DID — which route, which drive, which ground model and correction, where the
/// loads came from, and which executor produced each stage — never by the prose of
/// a warning: every `--exec gpu` run prints an `info:` line, so "a warning appeared"
/// says nothing. Read from the values the run used, not re-derived from its cards.
pub(super) struct DecisionInputs<'a> {
    pub(super) deck: &'a nec_model::deck::NecDeck,
    pub(super) segs: &'a [Segment],
    pub(super) solver_mode: SolverMode,
    pub(super) ground: &'a GroundModel,
    pub(super) ground_solver: GroundSolver,
    pub(super) sommerfeld_outcome: SommerfeldOutcome,
    pub(super) deck_loaded: bool,
    pub(super) config_loaded: bool,
    /// Whether the device filled the Z-matrix the solve used (in f32).
    pub(super) filled_on_device: bool,
    pub(super) solved_on_device: bool,
    pub(super) pattern_rows: usize,
    pub(super) pattern_on_device: bool,
    pub(super) near_field_points: usize,
}

/// `key=value` fields appended to the `diag:` line; see [`DecisionInputs`].
pub(super) fn decision_record(d: &DecisionInputs<'_>) -> String {
    let route = match d.solver_mode {
        SolverMode::Hallen => route_label(nec_solver::hallen_route(d.deck, d.segs)),
        // The order the sinusoidal arm solves in (FND-233).
        SolverMode::Sinusoidal => {
            let order = nec_solver::SegmentOrder::chains(d.segs);
            route_label(nec_solver::hallen_route(d.deck, &order.segments(d.segs)))
        }
        SolverMode::Mpie => "mpie",
        SolverMode::Pulse | SolverMode::Continuity => "pulse",
    };
    fn route_label(r: nec_solver::HallenRoute) -> &'static str {
        if r.unsupported_topology {
            "unsupported"
        } else if r.paths {
            "paths"
        } else {
            "plain"
        }
    }
    let drive = if deck_has_plane_wave(d.deck) {
        "planewave"
    } else if nec_solver::feedpoints(d.deck)
        .any(|(_, role)| role == nec_model::card::FeedpointRole::CurrentSource)
    {
        "current"
    } else {
        "voltage"
    };
    let ground = match d.ground {
        GroundModel::FreeSpace => "free",
        GroundModel::PerfectConductor => "pec",
        GroundModel::SimpleFiniteGround { .. } => "finite",
    };
    let gsolver = match (d.ground, d.ground_solver, d.sommerfeld_outcome) {
        // The MPIE fills the exact Sommerfeld kernels into its own matrix over a
        // finite ground, whatever `--ground-solver` says. It read `rcm` here — the
        // arm below did not ask the solver — naming the model it does not use
        // (FND-220).
        (GroundModel::SimpleFiniteGround { .. }, _, _) if d.solver_mode == SolverMode::Mpie => {
            "mpie-exact"
        }
        (_, _, SommerfeldOutcome::Applied) => "sommerfeld-applied",
        (_, _, SommerfeldOutcome::Declined) => "sommerfeld-declined",
        (_, _, SommerfeldOutcome::DeclinedReceive) => "sommerfeld-declined-receive",
        (GroundModel::SimpleFiniteGround { .. }, GroundSolver::Rcm, _) => "rcm",
        // No finite ground: no ground solver to name.
        _ => "n/a",
    };
    let loads = match (d.deck_loaded, d.config_loaded) {
        (false, false) => "none",
        (true, false) => "deck",
        (false, true) => "config",
        (true, true) => "deck+config",
    };
    let fill = if d.filled_on_device { "gpu" } else { "cpu" };
    let solve = if d.solved_on_device { "gpu" } else { "cpu" };
    let rp = match (d.pattern_rows, d.pattern_on_device) {
        (0, _) => "none",
        (_, true) => "gpu",
        (_, false) => "cpu",
    };
    let nf = if d.near_field_points == 0 {
        "none"
    } else {
        "cpu"
    };
    // A deck touching PEC solves as its image problem (FND-082) — a route of its
    // own for every output that must add the image (FND-201).
    let contact = matches!(d.ground, GroundModel::PerfectConductor)
        && nec_solver::ground_contact::touches_ground(d.segs);
    format!(
        "route={route} contact={} drive={drive} ground={ground} gsolver={gsolver} \
         loads={loads} fill_exec={fill} solve_exec={solve} rp_exec={rp} nf_exec={nf}",
        if contact { "yes" } else { "no" }
    )
}

/// What the sweep executor needs to know about one point's result: whether the
/// device solved it. Generic so the executor's tests need no real solve.
pub(super) trait LaneResult {
    fn ran_on_gpu(&self) -> bool;
}

impl LaneResult for Result<FrequencySolveResult, String> {
    fn ran_on_gpu(&self) -> bool {
        self.as_ref().is_ok_and(|p| p.ran_on_gpu)
    }
}

/// Peak memory of one sweep point's solve, as a multiple of one complex N×N
/// matrix (16·N² bytes), N = segments (FND-187). Measured 2026-10-03, one point,
/// `--exec cpu`, N = 1001, max RSS less the process's ~9 MB: hallen 4.02, hallen
/// over GN 2 4.06, hallen with the Sommerfeld correction 4.02, sinusoidal 6.54
/// (it keeps a Hallén system for its residual fallback), continuity 3.89, pulse
/// 3.03, mpie 3.03, mpie over GN 2 3.07. The worst case, rounded up.
const POINT_MATRICES: u64 = 7;
/// What the process holds besides its points (measured ~9 MB).
const PROCESS_FIXED_BYTES: u64 = 10_000_000;

/// How many sweep points may be in flight at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SweepBudget {
    pub threads: usize,
    pub per_point: u64,
}

/// The points in flight for `n_segs` segments within `budget` bytes, at most
/// `pool_threads` and never fewer than one. No budget, no cap.
pub(super) fn sweep_parallelism(
    n_segs: usize,
    budget: Option<u64>,
    pool_threads: usize,
) -> SweepBudget {
    let n = n_segs as u64;
    let per_point = POINT_MATRICES
        .saturating_mul(16)
        .saturating_mul(n.saturating_mul(n));
    let pool = pool_threads.max(1);
    let threads = match budget {
        None => pool,
        Some(b) => {
            let fit = b.saturating_sub(PROCESS_FIXED_BYTES) / per_point.max(1);
            usize::try_from(fit).unwrap_or(usize::MAX).clamp(1, pool)
        }
    };
    SweepBudget { threads, per_point }
}

/// The memory a sweep may use: `FNEC_SWEEP_MEMORY_BUDGET_MB` when set, else half
/// of what the host and the process's own cgroup leave available (Linux; `None`
/// elsewhere, which is no cap). `Err` names a malformed override.
pub(super) fn sweep_memory_budget() -> Result<Option<u64>, String> {
    if let Ok(v) = std::env::var("FNEC_SWEEP_MEMORY_BUDGET_MB") {
        return v
            .trim()
            .parse::<u64>()
            .map(|mb| Some(mb.saturating_mul(1_000_000)))
            .map_err(|_| {
                format!("FNEC_SWEEP_MEMORY_BUDGET_MB={v:?} is not a whole number of megabytes")
            });
    }
    Ok(available_memory().map(|b| b / 2))
}

/// `MemAvailable`, lowered to the process's cgroup v2 headroom when it has a
/// limit — a container's can be far below the host's free memory.
fn available_memory() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    let mut avail = kb.saturating_mul(1024);
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").ok();
    if let Some(path) = cgroup
        .as_deref()
        .and_then(|c| c.lines().find_map(|l| l.strip_prefix("0::")))
    {
        let dir = format!("/sys/fs/cgroup{}", path.trim());
        let read = |f: &str| std::fs::read_to_string(format!("{dir}/{f}")).ok();
        // `max` (no limit) does not parse, and leaves `avail` alone.
        if let (Some(max), Some(cur)) = (read("memory.max"), read("memory.current")) {
            if let (Ok(max), Ok(cur)) = (max.trim().parse::<u64>(), cur.trim().parse::<u64>()) {
                avail = avail.min(max.saturating_sub(cur));
            }
        }
    }
    Some(avail)
}

/// Execute a frequency sweep, at most `max_parallel` points in flight on the CPU.
///
/// - `cpu`: the points in parallel on the rayon pool, at most `max_parallel` at a
///   time — the memory budget of [`sweep_parallelism`] (FND-187).
/// - `gpu`: the points in turn on the one device.
/// - `hybrid` with `gpu_lane`: both at once. One dedicated thread solves points on
///   the device while every rayon worker solves points on the CPU, all pulling
///   the next index from one counter, so the split follows their real speeds.
///   A device point that comes back from the CPU means device trouble — the
///   caller only opens the lane for decks the device takes, with no stamps — so
///   the lane says so once and stops, rather than rebuilding a lost device for
///   every remaining point. Without `gpu_lane`, `hybrid` is `cpu`.
///
/// The GPU lane is a `std::thread`, never a rayon worker: its solve parks in
/// `pollster::block_on`, and a parked worker would shrink the pool.
///
/// Returns results indexed by frequency position, unsorted. The caller is
/// responsible for sorting by index and emitting per-result warnings.
#[allow(clippy::type_complexity)]
pub(super) fn execute_frequency_sweep<R, F>(
    freqs_hz: &[f64],
    execution_mode: ExecutionMode,
    gpu_lane: bool,
    max_parallel: usize,
    solve_one: F,
) -> Vec<(usize, R, u128)>
where
    R: LaneResult + Send,
    F: Fn(f64, ExecutionMode) -> R + Sync,
{
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let timed = |idx: usize, mode: ExecutionMode| {
        let t0 = std::time::Instant::now();
        let result = solve_one(freqs_hz[idx], mode);
        (idx, result, t0.elapsed().as_millis())
    };
    // CPU workers each take the next point from one counter, so at most this
    // many points are in flight on the CPU.
    let cpu_workers = rayon::current_num_threads().min(max_parallel).max(1);

    match execution_mode {
        // One device: a GPU sweep takes its points in turn.
        ExecutionMode::Gpu => (0..freqs_hz.len())
            .map(|i| timed(i, ExecutionMode::Gpu))
            .collect(),
        ExecutionMode::Hybrid if gpu_lane && freqs_hz.len() > 1 => {
            // The GPU lane holds a point too; keep one CPU worker regardless, or a
            // lane that retires on a fallback would leave the rest unsolved.
            let cpu_workers = cpu_workers.min(max_parallel.saturating_sub(1)).max(1);
            let next = AtomicUsize::new(0);
            let lane_open = AtomicBool::new(true);
            let solved = std::sync::Mutex::new(Vec::with_capacity(freqs_hz.len()));
            let take = || {
                let i = next.fetch_add(1, Ordering::Relaxed);
                (i < freqs_hz.len()).then_some(i)
            };
            let keep = |point| {
                solved
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(point);
            };
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    while lane_open.load(Ordering::Relaxed) {
                        let Some(i) = take() else { break };
                        let point = timed(i, ExecutionMode::Gpu);
                        if !point.1.ran_on_gpu() {
                            lane_open.store(false, Ordering::Relaxed);
                            eprintln!(
                                "warning: --exec hybrid: a point the GPU lane took came back \
                                 from the CPU, so the lane stopped; the rest of the sweep runs \
                                 on the CPU"
                            );
                        }
                        keep(point);
                    }
                });
                (0..cpu_workers).into_par_iter().for_each(|_| {
                    while let Some(i) = take() {
                        keep(timed(i, ExecutionMode::Cpu));
                    }
                });
            });
            solved
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
        // Every point is independent, and the caller re-sorts by index.
        _ => {
            let next = AtomicUsize::new(0);
            let solved = std::sync::Mutex::new(Vec::with_capacity(freqs_hz.len()));
            (0..cpu_workers).into_par_iter().for_each(|_| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= freqs_hz.len() {
                    break;
                }
                let point = timed(i, ExecutionMode::Cpu);
                solved
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(point);
            });
            solved
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }
}

/// The pulse solvers' right-hand-side scaling, as one function so the source
/// vector and the load diagonal cannot be scaled differently: a lumped load is a
/// source of `−Z_p·I_p` and must carry the source's units (FND-124).
fn scale_pulse_rhs(v: &[Complex64], mode: PulseRhsMode, freq_hz: f64) -> Vec<Complex64> {
    match mode {
        PulseRhsMode::Raw => v.to_vec(),
        PulseRhsMode::Nec2 => scale_excitation_for_pulse_rhs(v, freq_hz),
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::{execute_frequency_sweep, ExecutionMode, LaneResult};
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    /// A point's stand-in result: its frequency, and the lane that solved it.
    #[derive(Debug, Clone, Copy)]
    struct Point {
        freq: f64,
        mode: ExecutionMode,
        on_gpu: bool,
    }

    impl LaneResult for Point {
        fn ran_on_gpu(&self) -> bool {
            self.on_gpu
        }
    }

    const FREQS: [f64; 8] = [10e6, 11e6, 12e6, 13e6, 14e6, 15e6, 16e6, 17e6];

    fn in_pool<T: Send>(f: impl FnOnce() -> T + Send) -> T {
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("pool")
            .install(f)
    }

    /// Run a sweep whose first two arrivals rendezvous: the first waits up to
    /// 2 s for a second. Returns whether they met, and the points.
    fn sweep(
        mode: ExecutionMode,
        gpu_lane: bool,
        freqs: &[f64],
        device_ok: bool,
    ) -> (bool, Vec<(usize, Point)>) {
        sweep_within(mode, gpu_lane, freqs, device_ok, usize::MAX)
    }

    /// [`sweep`] with at most `max_parallel` points in flight on the CPU.
    fn sweep_within(
        mode: ExecutionMode,
        gpu_lane: bool,
        freqs: &[f64],
        device_ok: bool,
        max_parallel: usize,
    ) -> (bool, Vec<(usize, Point)>) {
        let arrived = Mutex::new(0usize);
        let cv = Condvar::new();
        let met = Mutex::new(false);
        let solved = in_pool(|| {
            execute_frequency_sweep(freqs, mode, gpu_lane, max_parallel, |freq, mode| {
                let mut n = arrived.lock().unwrap();
                *n += 1;
                if *n == 1 {
                    let (n, timeout) = cv
                        .wait_timeout_while(n, Duration::from_secs(2), |n| *n < 2)
                        .unwrap();
                    if !timeout.timed_out() && *n >= 2 {
                        *met.lock().unwrap() = true;
                    }
                } else {
                    cv.notify_all();
                    drop(n);
                }
                // A CPU point is slower than a device point, as on real decks
                // past the crossover; instant CPU points would let the pool
                // drain the sweep before the GPU thread had started.
                if mode == ExecutionMode::Cpu {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Point {
                    freq,
                    mode,
                    on_gpu: mode == ExecutionMode::Gpu && device_ok,
                }
            })
        });
        let met = *met.lock().unwrap();
        (met, solved.into_iter().map(|(i, p, _)| (i, p)).collect())
    }

    fn assert_each_point_once(freqs: &[f64], mut points: Vec<(usize, Point)>) -> Vec<Point> {
        points.sort_by_key(|p| p.0);
        assert_eq!(points.len(), freqs.len());
        points
            .into_iter()
            .enumerate()
            .map(|(i, (idx, p))| {
                assert_eq!(idx, i, "indices must be a permutation of 0..n");
                assert_eq!(p.freq, freqs[idx], "point {idx} solved the wrong frequency");
                p
            })
            .collect()
    }

    #[test]
    fn a_cpu_sweep_solves_its_points_concurrently_on_the_cpu() {
        let (met, points) = sweep(ExecutionMode::Cpu, false, &FREQS, true);
        assert!(
            met,
            "no two points were in flight at once: the sweep is sequential"
        );
        let points = assert_each_point_once(&FREQS, points);
        assert!(points.iter().all(|p| p.mode == ExecutionMode::Cpu));
    }

    /// FND-187: the memory budget bounds the points in flight. One slot: the
    /// first point waits its full 2 s and nobody arrives; two slots: they meet.
    #[test]
    fn a_cpu_sweep_with_one_slot_takes_its_points_in_turn() {
        let (met, points) = sweep_within(ExecutionMode::Cpu, false, &FREQS[..3], true, 1);
        assert!(!met, "one slot, yet two points were in flight at once");
        assert_each_point_once(&FREQS[..3], points);
        let (met, points) = sweep_within(ExecutionMode::Cpu, false, &FREQS, true, 2);
        assert!(met, "two slots, yet the points ran one at a time");
        assert_each_point_once(&FREQS, points);
    }

    /// A hybrid sweep with one slot keeps a CPU worker: a GPU lane that retires on
    /// its first fallback must not leave the remaining points unsolved.
    #[test]
    fn a_hybrid_sweep_with_one_slot_still_solves_every_point() {
        let (_, points) = sweep_within(ExecutionMode::Hybrid, true, &FREQS, false, 1);
        assert_each_point_once(&FREQS, points);
    }

    #[test]
    fn a_gpu_sweep_takes_its_points_in_turn() {
        // One device. The first point waits the full 2 s and nobody arrives.
        let (met, points) = sweep(ExecutionMode::Gpu, false, &FREQS[..2], true);
        assert!(!met, "a GPU sweep must not overlap its points");
        let points = assert_each_point_once(&FREQS[..2], points);
        assert!(points.iter().all(|p| p.mode == ExecutionMode::Gpu));
    }

    /// Both lanes take points, at once, and every point is solved exactly once.
    #[test]
    fn a_hybrid_sweep_runs_both_lanes_at_once() {
        let (met, points) = sweep(ExecutionMode::Hybrid, true, &FREQS, true);
        assert!(met, "no two points were in flight at once");
        let points = assert_each_point_once(&FREQS, points);
        let on_gpu = points
            .iter()
            .filter(|p| p.mode == ExecutionMode::Gpu)
            .count();
        assert!(on_gpu >= 1, "the GPU lane took no point");
        assert!(on_gpu < FREQS.len(), "the CPU pool took no point");
    }

    /// Without a GPU lane (an ineligible deck, no device), hybrid is the CPU.
    #[test]
    fn a_hybrid_sweep_without_a_gpu_lane_is_the_cpu() {
        let (_, points) = sweep(ExecutionMode::Hybrid, false, &FREQS, true);
        let points = assert_each_point_once(&FREQS, points);
        assert!(points.iter().all(|p| p.mode == ExecutionMode::Cpu));
    }

    /// A device point that came back from the CPU retires the lane: no point
    /// after it is sent to the device.
    #[test]
    fn a_failing_gpu_lane_stops_after_its_first_fallback() {
        let freqs: Vec<f64> = (0..64).map(|i| 1e6 * f64::from(i + 1)).collect();
        let (_, points) = sweep(ExecutionMode::Hybrid, true, &freqs, false);
        let points = assert_each_point_once(&freqs, points);
        let to_gpu = points
            .iter()
            .filter(|p| p.mode == ExecutionMode::Gpu)
            .count();
        assert_eq!(
            to_gpu, 1,
            "the lane kept pulling after a fallback: {to_gpu} points"
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::{sweep_parallelism, POINT_MATRICES, PROCESS_FIXED_BYTES};

    fn per_point(n: u64) -> u64 {
        POINT_MATRICES * 16 * n * n
    }

    #[test]
    fn the_budget_fits_whole_points_after_the_fixed_overhead() {
        let pp = per_point(1001);
        let b = |bytes| sweep_parallelism(1001, Some(bytes), 24).threads;
        assert_eq!(b(PROCESS_FIXED_BYTES + 3 * pp), 3);
        assert_eq!(
            b(PROCESS_FIXED_BYTES + 3 * pp - 1),
            2,
            "a partial point does not fit"
        );
        assert_eq!(sweep_parallelism(1001, Some(1), 24).per_point, pp);
    }

    #[test]
    fn never_fewer_than_one_nor_more_than_the_pool() {
        assert_eq!(sweep_parallelism(3001, Some(1), 24).threads, 1);
        assert_eq!(sweep_parallelism(11, Some(u64::MAX), 24).threads, 24);
        assert_eq!(sweep_parallelism(11, Some(u64::MAX), 0).threads, 1);
    }

    #[test]
    fn no_budget_is_no_cap() {
        assert_eq!(sweep_parallelism(3001, None, 24).threads, 24);
    }
}
