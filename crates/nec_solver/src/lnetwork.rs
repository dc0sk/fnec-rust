// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Lossless L-network matching (GAP-012 / BL-IMPR-017): the component values of
//! the two-element networks that present a target impedance at a feedpoint.
//!
//! An L-network is one series reactance and one shunt reactance, ideal and
//! lossless, in one of two orders. With the load `Z_L = R_L + jX_L`
//! (`Y_L = 1/Z_L = G_L + jB_L`) and the target `Z_T = R_T + jX_T`
//! (`Y_T = G_T + jB_T`), `ω = 2πf`:
//!
//! - **Series-first** (series `X_s` at the load, shunt `B_p` across the source
//!   side): `Z_1 = Z_L + jX_s = R_L + jX_t`, and `Y_in = 1/Z_1 + jB_p` must equal
//!   `Y_T`. The real part gives `R_L² + X_t² = R_L/G_T`, so the topology is
//!   feasible iff `R_L ≤ 1/G_T`, with `X_t = ±√(R_L/G_T − R_L²)`,
//!   `X_s = X_t − X_L` and `B_p = B_T + X_t·G_T/R_L`. `Q = |X_t|/R_L`.
//! - **Shunt-first** (shunt `B_p` across the load, series `X_s` toward the
//!   source): `Y_1 = Y_L + jB_p = G_L + jB_t`, and `Z_in = 1/Y_1 + jX_s` must
//!   equal `Z_T`. Feasible iff `G_L ≤ 1/R_T`, with `B_t = ±√(G_L/R_T − G_L²)`,
//!   `B_p = B_t − B_L` and `X_s = X_T + B_t·R_T/G_L`. `Q = |B_t|/G_L`.
//!
//! Both roots of both topologies give up to four networks. The two topologies
//! cannot both be infeasible for positive `R_L` and `R_T`: the first needs
//! `R_L·R_T > |Z_T|² ≥ R_T²`, the second `R_L·R_T > |Z_L|² ≥ R_L²`, and both
//! would need `R_L > R_T > R_L`. A one-element match (a root at exactly zero, as
//! `50 + j30 → 50`) comes out of both topologies as the same network and is
//! reported once. The input impedance of every solution is recomputed from its
//! component values, never copied from the target, so the printed `Z_IN` is a
//! check and not an echo.
//!
//! The network PRESENTS the target: `Z_in = Z_T`. For a conjugate match to a
//! source `Z_S`, pass `Z_S*` as the target.

use std::fmt;

use num_complex::Complex64;

/// Which element sits at the load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    /// Series reactance at the load, shunt across the source side.
    SeriesFirst,
    /// Shunt across the load, series reactance toward the source.
    ShuntFirst,
}

impl Topology {
    /// The CLI's spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            Topology::SeriesFirst => "series-first",
            Topology::ShuntFirst => "shunt-first",
        }
    }
}

/// One ideal lossless element, with the reactance it has at the design
/// frequency (the shunt element's own reactance, `−1/B`, not the susceptance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Element {
    Inductor { henry: f64, reactance_ohm: f64 },
    Capacitor { farad: f64, reactance_ohm: f64 },
}

impl Element {
    /// `L` or `C`.
    pub fn kind(&self) -> &'static str {
        match self {
            Element::Inductor { .. } => "L",
            Element::Capacitor { .. } => "C",
        }
    }

    /// Henry or farad.
    pub fn value(&self) -> f64 {
        match *self {
            Element::Inductor { henry, .. } => henry,
            Element::Capacitor { farad, .. } => farad,
        }
    }

    /// The element's reactance at the design frequency, Ω.
    pub fn reactance_ohm(&self) -> f64 {
        match *self {
            Element::Inductor { reactance_ohm, .. } | Element::Capacitor { reactance_ohm, .. } => {
                reactance_ohm
            }
        }
    }

    /// The element's impedance at `omega`, from its component value.
    pub fn impedance(&self, omega: f64) -> Complex64 {
        match *self {
            Element::Inductor { henry, .. } => Complex64::new(0.0, omega * henry),
            Element::Capacitor { farad, .. } => Complex64::new(0.0, -1.0 / (omega * farad)),
        }
    }

    /// The element with series reactance `x` at `omega`: an inductor for a
    /// positive reactance, a capacitor for a negative one.
    fn from_reactance(x: f64, omega: f64) -> Element {
        if x > 0.0 {
            Element::Inductor {
                henry: x / omega,
                reactance_ohm: x,
            }
        } else {
            Element::Capacitor {
                farad: -1.0 / (omega * x),
                reactance_ohm: x,
            }
        }
    }

    /// The element with shunt susceptance `b` at `omega`: a capacitor for a
    /// positive susceptance, an inductor for a negative one. Its reactance is
    /// `−1/b`.
    fn from_susceptance(b: f64, omega: f64) -> Element {
        if b > 0.0 {
            Element::Capacitor {
                farad: b / omega,
                reactance_ohm: -1.0 / b,
            }
        } else {
            Element::Inductor {
                henry: -1.0 / (omega * b),
                reactance_ohm: -1.0 / b,
            }
        }
    }
}

/// One matching network. `shunt`/`series` are `None` for a one-element match
/// (an element whose value the formulas put at zero reactance or susceptance,
/// within rounding — never a 1e-26 F capacitor).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solution {
    pub topology: Topology,
    pub shunt: Option<Element>,
    pub series: Option<Element>,
    /// Loaded Q of the match: `|X_t|/R_L` (series-first) or `|B_t|/G_L` (shunt-first).
    pub q: f64,
    /// The input impedance of this network terminated in the load, recomputed
    /// from the component values.
    pub z_in: Complex64,
}

/// Why no network can be given.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchError {
    /// `R_L ≤ 0`: a passive network cannot match it.
    NonPositiveLoadResistance(f64),
    /// `R_T ≤ 0`: not a passive target (a usage error at the CLI).
    NonPositiveTargetResistance(f64),
    /// `f ≤ 0` (a usage error at the CLI).
    NonPositiveFrequency(f64),
    /// A load, target or frequency that is not a finite number.
    NonFinite,
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchError::NonPositiveLoadResistance(r) => write!(
                f,
                "the feedpoint resistance is {r:.3} Ω; a passive network cannot match a \
                 negative or zero resistance"
            ),
            MatchError::NonPositiveTargetResistance(r) => write!(
                f,
                "the target resistance is {r} Ω; a matching network can only present a \
                 positive resistance"
            ),
            MatchError::NonPositiveFrequency(hz) => {
                write!(f, "the frequency is {} MHz; it must be positive", hz / 1e6)
            }
            MatchError::NonFinite => write!(f, "the load, target and frequency must be finite"),
        }
    }
}

impl std::error::Error for MatchError {}

/// Whether the load already is the target: `|Z_L − Z_T| ≤ 1e-9·|Z_T|`.
pub fn already_matched(z_load: Complex64, z_target: Complex64) -> bool {
    (z_load - z_target).norm() <= 1e-9 * z_target.norm()
}

/// The input impedance of a network of `shunt` and `series` in `topology`,
/// terminated in `z_load`, at `omega` — from the component values.
pub fn input_impedance(
    topology: Topology,
    shunt: Option<Element>,
    series: Option<Element>,
    z_load: Complex64,
    omega: f64,
) -> Complex64 {
    let z_series = series
        .map(|e| e.impedance(omega))
        .unwrap_or(Complex64::new(0.0, 0.0));
    let y_shunt = shunt
        .map(|e| e.impedance(omega).inv())
        .unwrap_or(Complex64::new(0.0, 0.0));
    match topology {
        Topology::SeriesFirst => {
            let z1 = z_load + z_series;
            (z1.inv() + y_shunt).inv()
        }
        Topology::ShuntFirst => {
            let y1 = z_load.inv() + y_shunt;
            y1.inv() + z_series
        }
    }
}

/// Relative size below which a computed reactance or susceptance is the
/// rounding residue of an exact zero, and the element is left out.
const ELEMENT_EPS: f64 = 1e-12;

/// Relative tolerance under which two solutions are one network.
const SAME_EPS: f64 = 1e-9;

/// Every lossless L-network that presents `z_target` at a feedpoint of `z_load`
/// at `f_hz`: both topologies, both roots, deduplicated, up to four. An empty
/// list means the load already is the target (see [`already_matched`]); for a
/// positive load resistance that is the only way to get none.
pub fn solve(
    z_load: Complex64,
    z_target: Complex64,
    f_hz: f64,
) -> Result<Vec<Solution>, MatchError> {
    if !z_load.is_finite() || !z_target.is_finite() || !f_hz.is_finite() {
        return Err(MatchError::NonFinite);
    }
    if f_hz <= 0.0 {
        return Err(MatchError::NonPositiveFrequency(f_hz));
    }
    if z_target.re <= 0.0 {
        return Err(MatchError::NonPositiveTargetResistance(z_target.re));
    }
    if z_load.re <= 0.0 {
        return Err(MatchError::NonPositiveLoadResistance(z_load.re));
    }
    if already_matched(z_load, z_target) {
        return Ok(Vec::new());
    }

    let omega = 2.0 * std::f64::consts::PI * f_hz;
    let (r_l, x_l) = (z_load.re, z_load.im);
    let (r_t, x_t_target) = (z_target.re, z_target.im);
    let y_l = z_load.inv();
    let y_t = z_target.inv();
    let (g_l, b_l) = (y_l.re, y_l.im);
    let (g_t, b_t_target) = (y_t.re, y_t.im);

    let series_of = |x_s: f64| {
        (x_s.abs() > ELEMENT_EPS * z_load.norm()).then(|| Element::from_reactance(x_s, omega))
    };
    let shunt_of = |b_p: f64| {
        (b_p.abs() > ELEMENT_EPS * y_l.norm()).then(|| Element::from_susceptance(b_p, omega))
    };

    let mut out: Vec<Solution> = Vec::new();
    let mut push = |topology: Topology, shunt: Option<Element>, series: Option<Element>, q: f64| {
        let candidate = Solution {
            topology,
            shunt,
            series,
            q,
            z_in: input_impedance(topology, shunt, series, z_load, omega),
        };
        if !out.iter().any(|s| same_network(s, &candidate)) {
            out.push(candidate);
        }
    };

    // Series-first: feasible iff R_L ≤ 1/G_T. The radicand is exactly zero for a
    // one-element match and may round to either side of it.
    let scale_a = r_l / g_t;
    let radicand_a = scale_a - r_l * r_l;
    if radicand_a >= -ELEMENT_EPS * scale_a {
        let root = radicand_a.max(0.0).sqrt();
        for x_t in [root, -root] {
            let x_s = x_t - x_l;
            let b_p = b_t_target + x_t * g_t / r_l;
            push(
                Topology::SeriesFirst,
                shunt_of(b_p),
                series_of(x_s),
                x_t.abs() / r_l,
            );
        }
    }

    // Shunt-first: feasible iff G_L ≤ 1/R_T.
    let scale_b = g_l / r_t;
    let radicand_b = scale_b - g_l * g_l;
    if radicand_b >= -ELEMENT_EPS * scale_b {
        let root = radicand_b.max(0.0).sqrt();
        for b_t in [root, -root] {
            let b_p = b_t - b_l;
            let x_s = x_t_target + b_t * r_t / g_l;
            push(
                Topology::ShuntFirst,
                shunt_of(b_p),
                series_of(x_s),
                b_t.abs() / g_l,
            );
        }
    }

    Ok(out)
}

/// Two solutions are one network when their elements agree in kind and value.
/// With both elements present the order matters too (the shunt is at the
/// source side in one topology and at the load in the other); a one-element
/// network has no order.
fn same_network(a: &Solution, b: &Solution) -> bool {
    let same_element = |x: Option<Element>, y: Option<Element>| match (x, y) {
        (None, None) => true,
        (Some(p), Some(q)) => {
            p.kind() == q.kind()
                && (p.value() - q.value()).abs() <= SAME_EPS * p.value().abs().max(q.value().abs())
        }
        _ => false,
    };
    if !(same_element(a.shunt, b.shunt) && same_element(a.series, b.series)) {
        return false;
    }
    let two_elements = a.shunt.is_some() && a.series.is_some();
    !two_elements || a.topology == b.topology
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MHZ: f64 = 1e6;

    fn z(re: f64, im: f64) -> Complex64 {
        Complex64::new(re, im)
    }

    fn rel_eq(a: f64, b: f64, tol: f64, what: &str) {
        assert!(
            (a - b).abs() <= tol * b.abs().max(1e-300),
            "{what}: got {a:e}, expected {b:e} (rel tol {tol:e})"
        );
    }

    /// `(shunt kind, shunt value, series kind, series value)` of a solution,
    /// `"-"`/0 for an absent element.
    fn components(s: &Solution) -> (&'static str, f64, &'static str, f64) {
        let el = |e: Option<Element>| e.map(|e| (e.kind(), e.value())).unwrap_or(("-", 0.0));
        let (sk, sv) = el(s.shunt);
        let (rk, rv) = el(s.series);
        (sk, sv, rk, rv)
    }

    /// Every solution's recomputed input impedance is the target.
    fn assert_all_present_target(sols: &[Solution], target: Complex64, tol: f64) {
        for s in sols {
            assert!(
                (s.z_in - target).norm() <= tol,
                "{s:?}: z_in {} is not the target {target}",
                s.z_in
            );
        }
    }

    fn find(sols: &[Solution], sk: &str, sv: f64, rk: &str, rv: f64) -> Solution {
        *sols
            .iter()
            .find(|s| {
                let (a, b, c, d) = components(s);
                a == sk
                    && c == rk
                    && (b - sv).abs() <= 1e-5 * sv.abs().max(1e-300)
                    && (d - rv).abs() <= 1e-5 * rv.abs().max(1e-300)
            })
            .unwrap_or_else(|| {
                panic!("no solution shunt {sk} {sv:e} series {rk} {rv:e} in {sols:#?}")
            })
    }

    /// 25 Ω → 50 Ω at 14.2 MHz: two series-first networks, Q = 1; shunt-first
    /// is infeasible (G_L = 0.04 > 1/R_T).
    #[test]
    fn real_load_below_target_gives_two_series_first_networks() {
        let sols = solve(z(25.0, 0.0), z(50.0, 0.0), 14.2 * MHZ).unwrap();
        assert_eq!(sols.len(), 2, "{sols:#?}");
        assert!(sols.iter().all(|s| s.topology == Topology::SeriesFirst));
        let a = find(&sols, "C", 2.241619e-10, "L", 2.802024e-7);
        let b = find(&sols, "L", 5.604047e-7, "C", 4.483238e-10);
        rel_eq(a.series.unwrap().reactance_ohm(), 25.0, 1e-9, "X_series");
        rel_eq(a.shunt.unwrap().reactance_ohm(), -50.0, 1e-9, "X_shunt");
        rel_eq(b.series.unwrap().reactance_ohm(), -25.0, 1e-9, "X_series");
        rel_eq(b.shunt.unwrap().reactance_ohm(), 50.0, 1e-9, "X_shunt");
        rel_eq(a.q, 1.0, 1e-9, "Q");
        assert_all_present_target(&sols, z(50.0, 0.0), 1e-9);
    }

    /// 73 + j42 → 50 at 14.2 MHz: two shunt-first networks; at 7.1 MHz the
    /// reactances are the same and every L and C doubles.
    #[test]
    fn complex_load_above_target_gives_two_shunt_first_networks() {
        let sols = solve(z(73.0, 42.0), z(50.0, 0.0), 14.2 * MHZ).unwrap();
        assert_eq!(sols.len(), 2, "{sols:#?}");
        assert!(sols.iter().all(|s| s.topology == Topology::ShuntFirst));
        let a = find(&sols, "C", 1.784000e-10, "L", 5.442819e-7);
        let b = find(&sols, "L", 2.750851e-6, "C", 2.308021e-10);
        rel_eq(a.shunt.unwrap().reactance_ohm(), -62.8256, 1e-5, "X_shunt");
        rel_eq(a.series.unwrap().reactance_ohm(), 48.5615, 1e-5, "X_series");
        rel_eq(b.shunt.unwrap().reactance_ohm(), 245.4343, 1e-5, "X_shunt");
        rel_eq(
            b.series.unwrap().reactance_ohm(),
            -48.5615,
            1e-5,
            "X_series",
        );
        rel_eq(a.q, 0.971, 1e-3, "Q");
        assert_all_present_target(&sols, z(50.0, 0.0), 1e-9);

        let half = solve(z(73.0, 42.0), z(50.0, 0.0), 7.1 * MHZ).unwrap();
        assert_eq!(half.len(), 2);
        find(&half, "C", 3.568001e-10, "L", 1.088564e-6);
        find(&half, "L", 5.501702e-6, "C", 4.616041e-10);
        assert_all_present_target(&half, z(50.0, 0.0), 1e-9);
    }

    /// 200 − j100 → 50 at 28.5 MHz.
    #[test]
    fn capacitive_high_load() {
        let sols = solve(z(200.0, -100.0), z(50.0, 0.0), 28.5 * MHZ).unwrap();
        assert_eq!(sols.len(), 2, "{sols:#?}");
        let a = find(&sols, "C", 3.350630e-11, "L", 5.584384e-7);
        rel_eq(a.shunt.unwrap().reactance_ohm(), -166.6667, 1e-5, "X_shunt");
        rel_eq(a.series.unwrap().reactance_ohm(), 100.0, 1e-9, "X_series");
        find(&sols, "L", 5.584384e-7, "C", 5.584384e-11);
        assert_all_present_target(&sols, z(50.0, 0.0), 1e-9);
    }

    /// 50 + j30 → 50: the series-first radicand is exactly zero, so one network
    /// is a single series capacitor (from both topologies, reported once), and
    /// the other is a shunt C with a series L. No component is a rounding residue.
    #[test]
    fn one_element_match_is_reported_once_with_no_residue_element() {
        let sols = solve(z(50.0, 30.0), z(50.0, 0.0), 14.2 * MHZ).unwrap();
        assert_eq!(sols.len(), 2, "{sols:#?}");
        let alone = find(&sols, "-", 0.0, "C", 3.736032e-10);
        assert!(alone.shunt.is_none());
        rel_eq(
            alone.series.unwrap().reactance_ohm(),
            -30.0,
            1e-9,
            "X_series",
        );
        find(&sols, "C", 1.977899e-10, "L", 3.362428e-7);
        for s in &sols {
            for e in [s.shunt, s.series].into_iter().flatten() {
                assert!(e.value() > 1e-20, "residue component {e:?}");
            }
        }
        assert_all_present_target(&sols, z(50.0, 0.0), 1e-9);
    }

    /// 5 − j300 → 50 at 3.6 MHz: both topologies feasible, four networks.
    #[test]
    fn low_resistance_capacitive_load_gives_four_networks() {
        let sols = solve(z(5.0, -300.0), z(50.0, 0.0), 3.6 * MHZ).unwrap();
        assert_eq!(sols.len(), 4, "{sols:#?}");
        let a1 = find(&sols, "C", 2.652582e-9, "L", 1.392606e-5);
        let a2 = find(&sols, "L", 7.368284e-7, "L", 1.259977e-5);
        let b1 = find(&sols, "L", 1.939085e-5, "L", 4.188855e-5);
        let b2 = find(&sols, "L", 1.008229e-5, "C", 4.665948e-11);
        assert_eq!(a1.topology, Topology::SeriesFirst);
        assert_eq!(a2.topology, Topology::SeriesFirst);
        assert_eq!(b1.topology, Topology::ShuntFirst);
        assert_eq!(b2.topology, Topology::ShuntFirst);
        assert_all_present_target(&sols, z(50.0, 0.0), 1e-9);
    }

    /// A complex target is presented exactly, including its reactance.
    #[test]
    fn complex_target_is_presented() {
        let target = z(25.0, 10.0);
        let sols = solve(z(73.0, 42.0), target, 14.2 * MHZ).unwrap();
        assert!(!sols.is_empty());
        assert_all_present_target(&sols, target, 1e-9);
        let sols = solve(z(25.0, 0.0), z(50.0, -20.0), 14.2 * MHZ).unwrap();
        assert!(!sols.is_empty());
        assert_all_present_target(&sols, z(50.0, -20.0), 1e-9);
    }

    #[test]
    fn already_matched_load_gives_no_network() {
        assert!(already_matched(z(50.0, 0.0), z(50.0, 0.0)));
        assert!(already_matched(z(50.0 + 1e-9, 0.0), z(50.0, 0.0)));
        assert!(!already_matched(z(50.0, 1e-6), z(50.0, 0.0)));
        assert_eq!(
            solve(z(50.0, 0.0), z(50.0, 0.0), 14.2 * MHZ).unwrap(),
            vec![]
        );
    }

    #[test]
    fn refusals() {
        assert_eq!(
            solve(z(-2.117, -444.86), z(50.0, 0.0), 14.2 * MHZ),
            Err(MatchError::NonPositiveLoadResistance(-2.117))
        );
        assert_eq!(
            solve(z(0.0, 100.0), z(50.0, 0.0), 14.2 * MHZ),
            Err(MatchError::NonPositiveLoadResistance(0.0))
        );
        assert_eq!(
            solve(z(73.0, 0.0), z(0.0, 0.0), 14.2 * MHZ),
            Err(MatchError::NonPositiveTargetResistance(0.0))
        );
        assert_eq!(
            solve(z(73.0, 0.0), z(50.0, 0.0), 0.0),
            Err(MatchError::NonPositiveFrequency(0.0))
        );
        assert_eq!(
            solve(z(f64::NAN, 0.0), z(50.0, 0.0), 1.0),
            Err(MatchError::NonFinite)
        );
        let msg = MatchError::NonPositiveLoadResistance(-2.117).to_string();
        assert!(
            msg.contains("-2.117 Ω") && msg.contains("cannot match"),
            "{msg}"
        );
    }

    proptest! {
        /// Every network returned presents the target, from its component
        /// values, for any positive-resistance load and target; and there is
        /// always at least one unless the load already is the target.
        #[test]
        fn every_network_presents_the_target(
            r_l in 0.1f64..5000.0, x_l in -5000.0f64..5000.0,
            r_t in 1.0f64..600.0, x_t in -300.0f64..300.0,
            f_mhz in 0.1f64..3000.0,
        ) {
            let (load, target) = (z(r_l, x_l), z(r_t, x_t));
            let sols = solve(load, target, f_mhz * MHZ).unwrap();
            prop_assert!(!sols.is_empty() || already_matched(load, target));
            prop_assert!(sols.len() <= 4);
            for s in &sols {
                prop_assert!(
                    (s.z_in - target).norm() <= 1e-7 * target.norm(),
                    "{s:?} does not present {target}"
                );
                for e in [s.shunt, s.series].into_iter().flatten() {
                    prop_assert!(e.value() > 0.0 && e.value().is_finite(), "{e:?}");
                }
            }
        }
    }
}
