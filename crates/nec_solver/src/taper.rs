// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Leeson step-tapered-radius correction (BL-IMPR-014).
//!
//! NEC-2-class cores mis-model an element built from several tubing diameters
//! (a "step-tapered" element) — they over-report gain and under-report feed
//! impedance. D. B. Leeson (W6QHS), *Physical Design of Yagi Antennas* (ARRL,
//! 1992), ch. 8, § 8.4, gives an algorithm that replaces the tapered element
//! with an **equivalent uniform-diameter cylinder** (a corrected half-length and
//! radius) that has the same self-impedance near resonance. It is a geometry
//! preprocessing step — the solver is unchanged; it just sees the substitute
//! element. Valid for linear, essentially unloaded elements within ~±15 % of
//! self-resonance (the algorithm assumes a sinusoidal current, so `2βxₙ = πxₙ/ℓ`
//! at resonance).
//!
//! This module is a direct transcription of the book's equations 8-44…8-59 and
//! Tables 8-2/8-3; the unit test reproduces the book's worked example
//! (ℓ′ = 95.70 in, d′ = 0.594 in) to the digit.
//!
//! Units are the caller's own (radii and lengths must share one unit); the
//! result is in the same unit — the algorithm depends only on the ratio `2ℓ/aₑ`.

/// Cylinder self-impedance constants at resonance (βℓ = π/2), from ch. 8:
/// `M_cyl(π/2) = 60(Cin π − 2)`, `Xₐ(π/2) − N_cyl(π/2) = 30 Si(2π)`,
/// `dM_cyl/d(βℓ) = 240/π`, and `d(Xₐ − N)_cyl/d(βℓ) = −56.93` Ω/rad.
const M_CYL: f64 = -21.13;
const XA_MINUS_N_CYL: f64 = 42.545;
const DM_CYL: f64 = 76.39; // 240/π
const DXAN_CYL: f64 = 56.93; // magnitude of d(Xₐ−N)_cyl/d(βℓ); enters as (56.93 + ΔN′)

/// One section of a step-tapered *half*-element, ordered from the feed (centre)
/// outward toward the tip. `radius` and `length` share the caller's unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaperSection {
    pub radius: f64,
    pub length: f64,
}

/// The uniform-cylinder element equivalent to a step-tapered one, plus the key
/// intermediates for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquivalentElement {
    /// Corrected half-length ℓ′ of the substitute cylinder (Eq 8-57).
    pub half_length: f64,
    /// Corrected radius a′ of the substitute cylinder (Eq 8-59).
    pub radius: f64,
    /// First-order logarithmic-mean radius aₑ (Eq 8-44).
    pub equiv_radius_first_order: f64,
    /// Average characteristic impedance Kₐ of the tapered element (Eq 8-47), Ω.
    pub k_a: f64,
    /// Effective characteristic impedance Z₀ of the tapered element, Ω.
    pub z0: f64,
    /// Half-wave reactance X₀ of the tapered element, Ω.
    pub x0: f64,
}

/// Compute the equivalent uniform cylinder for a step-tapered half-element
/// (`sections` ordered centre → tip). Returns an error for empty input or a
/// non-positive radius/length.
pub fn leeson_equivalent_element(sections: &[TaperSection]) -> Result<EquivalentElement, String> {
    if sections.is_empty() {
        return Err("taper correction needs at least one section".to_string());
    }
    for (i, s) in sections.iter().enumerate() {
        if !s.radius.is_finite() || s.radius <= 0.0 || !s.length.is_finite() || s.length <= 0.0 {
            return Err(format!(
                "taper section {i} must have positive radius and length (got r={}, l={})",
                s.radius, s.length
            ));
        }
    }

    let l: f64 = sections.iter().map(|s| s.length).sum(); // total half-length ℓ

    // Eq 8-44: ln aₑ = Σ (ℓₙ/ℓ) ln aₙ  (length-weighted log-mean radius).
    let ln_ae: f64 = sections
        .iter()
        .map(|s| (s.length / l) * s.radius.ln())
        .sum();
    let a_e = ln_ae.exp();

    // Eq 8-47: Kₐ = 120 (ln 2ℓ − ln aₑ − 1).
    let k_a = 120.0 * ((2.0 * l).ln() - ln_ae - 1.0);

    // Tapered corrections ΔN, ΔM and derivatives ΔN′, ΔM′ (Table 8-2). At
    // resonance the phase is 2βxₙ = πxₙ/ℓ.
    let (mut d_n, mut d_m, mut d_np, mut d_mp) = (0.0, 0.0, 0.0, 0.0);
    let mut x_prev = 0.0_f64; // x₀ = 0
    for s in sections {
        let x_n = x_prev + s.length;
        let ln_ratio = s.radius.ln() - ln_ae; // ln(aₙ/aₑ)
        let (pn, pp) = (
            std::f64::consts::PI * x_n / l,
            std::f64::consts::PI * x_prev / l,
        );
        d_n += 60.0 * ln_ratio * (pn.sin() - pp.sin());
        d_m += -60.0 * ln_ratio * (pn.cos() - pp.cos());
        d_np += (120.0 / l) * ln_ratio * (x_n * pn.cos() - x_prev * pp.cos());
        d_mp += (120.0 / l) * ln_ratio * (x_n * pn.sin() - x_prev * pp.sin());
        x_prev = x_n;
    }

    // Table 8-3, tapered element:
    let m = M_CYL + d_m;
    let x0 = (XA_MINUS_N_CYL - d_n) * (1.0 - m / k_a); // Eq for X₀
    let z0 = k_a - 2.0 * m - (1.0 - m / k_a) * (DXAN_CYL + d_np) - (x0 / k_a) * (DM_CYL + d_mp);

    // Equivalent cylinder (Eq 8-54…8-59):
    let x0_cyl = XA_MINUS_N_CYL * (1.0 + 21.13 / k_a); // X₀′  (Eq 8-54)
    let f_ratio = 1.0 - 2.0 * (x0 - x0_cyl) / (std::f64::consts::PI * z0); // f₀′/f₀ (Eq 8-55)
    let z0_cyl = z0 * f_ratio; // Z₀′ (Eq 8-56)
    let l_prime = l / f_ratio; // ℓ′  (Eq 8-57)
    let k_a_cyl = z0_cyl + 14.67 + 4253.0 / k_a + 68673.0 / (k_a * k_a); // Kₐ′ (Eq 8-58)
                                                                         // Eq 8-59: ln a′ = ln(2ℓ′) − Kₐ′/120 − 1.
    let a_prime = ((2.0 * l_prime).ln() - k_a_cyl / 120.0 - 1.0).exp();

    Ok(EquivalentElement {
        half_length: l_prime,
        radius: a_prime,
        equiv_radius_first_order: a_e,
        k_a,
        z0,
        x0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64, what: &str) {
        assert!(
            (a - b).abs() < tol,
            "{what}: got {a}, expected {b} (tol {tol})"
        );
    }

    /// Reproduces the book's worked example (Tables 8-2/8-3): a 100-unit
    /// half-element of two 50-unit sections, diameters 0.8 and 0.4
    /// (radii 0.4 and 0.2), centre → tip. Expected equivalent cylinder:
    /// ℓ′ = 95.70, d′ = 0.594 (a′ = 0.297).
    #[test]
    fn reproduces_leeson_worked_example() {
        let sections = [
            TaperSection {
                radius: 0.4,
                length: 50.0,
            }, // centre (thick)
            TaperSection {
                radius: 0.2,
                length: 50.0,
            }, // tip (thin)
        ];
        let e = leeson_equivalent_element(&sections).unwrap();
        // First-order + characteristic impedances (Table 8-3).
        approx(e.equiv_radius_first_order.ln(), -1.26, 0.01, "ln aₑ");
        approx(e.k_a, 667.0, 1.0, "Kₐ");
        approx(e.x0, 0.99, 0.05, "X₀");
        approx(e.z0, 608.0, 1.5, "Z₀");
        // Equivalent cylinder dimensions (the deliverable).
        approx(e.half_length, 95.70, 0.05, "ℓ′");
        approx(2.0 * e.radius, 0.594, 0.002, "d′ = 2a′");
    }

    /// A uniform element (single section, or all sections equal radius) must map
    /// to essentially itself: ℓ′ ≈ ℓ and a′ ≈ a.
    #[test]
    fn uniform_element_is_near_identity() {
        let e = leeson_equivalent_element(&[TaperSection {
            radius: 0.3,
            length: 100.0,
        }])
        .unwrap();
        approx(e.half_length, 100.0, 0.5, "ℓ′ ≈ ℓ");
        approx(e.radius, 0.3, 0.01, "a′ ≈ a");
        approx(e.equiv_radius_first_order, 0.3, 1e-9, "aₑ = a");
    }

    /// The equivalent radius must sit between the thinnest and thickest section,
    /// and (for a thick-centre/thin-tip taper) the substitute is shorter than the
    /// physical half-length.
    #[test]
    fn equivalent_radius_brackets_sections() {
        let e = leeson_equivalent_element(&[
            TaperSection {
                radius: 0.5,
                length: 40.0,
            },
            TaperSection {
                radius: 0.3,
                length: 30.0,
            },
            TaperSection {
                radius: 0.15,
                length: 30.0,
            },
        ])
        .unwrap();
        assert!(
            e.radius > 0.15 && e.radius < 0.5,
            "a′={} not bracketed",
            e.radius
        );
        assert!(
            e.half_length < 100.0,
            "ℓ′={} not < physical 100",
            e.half_length
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(leeson_equivalent_element(&[]).is_err());
        assert!(leeson_equivalent_element(&[TaperSection {
            radius: 0.0,
            length: 1.0
        }])
        .is_err());
        assert!(leeson_equivalent_element(&[TaperSection {
            radius: 0.1,
            length: -1.0
        }])
        .is_err());
    }
}
