// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The sections a sweep closes with, after the per-frequency blocks:
//! `SWEEP_POINTS` and `INPUT_IMPEDANCE` (FND-309).
//!
//! One renderer for every route that solves a sweep — the local run and the
//! distributed one called two copy-pasted `SWEEP_POINTS` emitters before this,
//! the "N copies, one diverged" shape. Its input is what both routes have: per
//! point, the frequency and the feedpoint impedances in FEEDPOINTS order.
//!
//! This module renders; it does not decide which source the input-impedance
//! table follows or what it is normalized to. That is
//! `nec_solver::input_impedance_print`'s, read from the deck, and arrives here as
//! an [`InputImpedanceRequest`].

use num_complex::Complex64;

/// One feedpoint of one solved point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepFeed {
    /// The resolved segment's tag, as FEEDPOINTS prints it.
    pub tag: usize,
    /// The segment's index within its tag.
    pub seg: usize,
    /// The segment's absolute number, from 1 — nec2c's SOURCE SEGMENT No.
    pub abs_seg: usize,
    /// The feedpoint impedance, ohms.
    pub z: Complex64,
}

/// One solved frequency of a sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepPoint {
    pub freq_mhz: f64,
    /// Every feedpoint, in the report's FEEDPOINTS order; never empty.
    pub feeds: Vec<SweepFeed>,
}

/// What the input-impedance table divides by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputImpedanceNorm {
    /// A fixed impedance, ohms (EX F3 > 0).
    Fixed(f64),
    /// The largest `|Z|` over the sweep (EX F3 = 0).
    Max,
}

/// A deck's request for the input-impedance table: the feedpoint it follows,
/// by its absolute segment number ([`SweepFeed::abs_seg`], nec2c's SOURCE
/// SEGMENT No), and its normalization. Keyed by segment, not by row index: the
/// rows are one per driven segment, so a card NEC-2 overrides has none and an
/// index counted over the cards points past the source (FND-305).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputImpedanceRequest {
    pub abs_seg: usize,
    pub norm: InputImpedanceNorm,
}

/// The rendered sections, and why a requested table was not printed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SweepSections {
    /// The sections, each after a blank line; empty for a single point.
    pub text: String,
    /// Warnings for stderr.
    pub warnings: Vec<String>,
}

/// Render `SWEEP_POINTS` (for two or more points) and, when `request` asks for
/// it, `INPUT_IMPEDANCE`.
///
/// The table is printed only for two or more solved points — nec2c prints none
/// for a single frequency — and never over a partial sweep: with any point in
/// `failed_mhz` it is omitted with a warning naming the failed frequencies, as a
/// largest-|Z| factor over the points that solved is not nec2c's factor and a
/// short table reads as complete.
pub fn render_sweep_sections(
    points: &[SweepPoint],
    request: Option<InputImpedanceRequest>,
    failed_mhz: &[f64],
) -> SweepSections {
    let mut out = SweepSections::default();
    if points.len() > 1 {
        // Bit for bit what both routes printed before (report_contract.rs).
        out.text.push('\n');
        out.text.push_str("SWEEP_POINTS\n");
        out.text.push_str(&format!("N_POINTS {}\n", points.len()));
        out.text.push_str("FREQ_MHZ TAG SEG Z_RE Z_IM\n");
        for p in points {
            if let Some(f) = p.feeds.first() {
                out.text.push_str(&format!(
                    "{:.6} {} {} {:.6} {:.6}\n",
                    p.freq_mhz, f.tag, f.seg, f.z.re, f.z.im
                ));
            }
        }
    }
    let Some(request) = request else {
        return out;
    };
    if !failed_mhz.is_empty() {
        let list = failed_mhz
            .iter()
            .map(|f| format!("{f:.6}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.warnings.push(format!(
            "EX I4's input-impedance print (INPUT_IMPEDANCE) omitted: the point(s) at {list} \
             MHz failed, and a table over a partial sweep is not the one NEC-2 prints"
        ));
        return out;
    }
    if points.len() < 2 {
        // nec2c prints no table for a single frequency (main.c, nfrq == 1).
        return out;
    }
    let Some(chosen) = points
        .iter()
        .map(|p| {
            p.feeds
                .iter()
                .find(|f| f.abs_seg == request.abs_seg)
                .copied()
        })
        .collect::<Option<Vec<SweepFeed>>>()
    else {
        out.warnings.push(format!(
            "EX I4's input-impedance print (INPUT_IMPEDANCE) omitted: a point has no \
             feedpoint row on segment {} to report",
            request.abs_seg
        ));
        return out;
    };
    let (factor, label) = match request.norm {
        InputImpedanceNorm::Fixed(f) => (f, "F3"),
        InputImpedanceNorm::Max => (
            chosen.iter().map(|f| f.z.norm()).fold(0.0_f64, f64::max),
            "MAX",
        ),
    };
    let src = chosen[0];
    out.text.push('\n');
    out.text.push_str("INPUT_IMPEDANCE\n");
    out.text.push_str(&format!(
        "SOURCE_TAG {} SOURCE_SEG {} SOURCE_SEGMENT {}\n",
        src.tag, src.seg, src.abs_seg
    ));
    out.text.push_str(&format!(
        "NORMALIZATION_OHMS {factor:.6} NORMALIZED_TO {label}\n"
    ));
    out.text.push_str(&format!("N_POINTS {}\n", points.len()));
    out.text
        .push_str("FREQ_MHZ Z_RE Z_IM Z_ABS Z_PHASE_DEG ZN_RE ZN_IM ZN_ABS\n");
    for (p, f) in points.iter().zip(&chosen) {
        // Only the magnitude is normalized; the phase is Z's own, as nec2c
        // prints it.
        let zn = f.z / factor;
        out.text.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6e} {:.6e} {:.6e}\n",
            p.freq_mhz,
            f.z.re,
            f.z.im,
            f.z.norm(),
            f.z.arg().to_degrees(),
            zn.re,
            zn.im,
            zn.norm()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(freq_mhz: f64, zs: &[(f64, f64)]) -> SweepPoint {
        SweepPoint {
            freq_mhz,
            feeds: zs
                .iter()
                .enumerate()
                .map(|(i, &(re, im))| SweepFeed {
                    tag: i + 1,
                    seg: 6,
                    abs_seg: 6 + 11 * i,
                    z: Complex64::new(re, im),
                })
                .collect(),
        }
    }

    fn rows(text: &str) -> Vec<Vec<f64>> {
        let start = text.find("INPUT_IMPEDANCE\n").expect("the section");
        text[start..]
            .lines()
            .skip(5)
            .take_while(|l| !l.is_empty())
            .map(|l| l.split(' ').map(|c| c.parse().unwrap()).collect())
            .collect()
    }

    /// nec2c's `max` deck numbers: 28/29/30 MHz, factor = the 30 MHz |Z|.
    fn max_sweep() -> Vec<SweepPoint> {
        vec![
            point(28.0, &[(65.115, -47.409), (1.0, 0.0)]),
            point(29.0, &[(71.926, -0.73058), (1.0, 0.0)]),
            point(30.0, &[(79.393, 45.571), (1.0, 0.0)]),
        ]
    }

    #[test]
    fn a_fixed_factor_divides_every_point_and_leaves_the_phase() {
        let pts = vec![
            point(29.9, &[(78.614, 40.951)]),
            point(30.9, &[(86.740, 87.106)]),
        ];
        let req = InputImpedanceRequest {
            abs_seg: 6,
            norm: InputImpedanceNorm::Fixed(50.0),
        };
        let s = render_sweep_sections(&pts, Some(req), &[]);
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert!(s.text.contains(
            "\nINPUT_IMPEDANCE\nSOURCE_TAG 1 SOURCE_SEG 6 SOURCE_SEGMENT 6\n\
             NORMALIZATION_OHMS 50.000000 NORMALIZED_TO F3\nN_POINTS 2\n"
        ));
        let r = rows(&s.text);
        assert_eq!(r.len(), 2);
        // nec2c: 1.5723 + j0.81903 at 29.9 MHz, phase 27.52 degrees.
        assert!((r[0][5] - 1.5723).abs() < 1e-4 && (r[0][6] - 0.81903).abs() < 1e-4);
        assert!((r[0][4] - 27.52).abs() < 0.01, "{}", r[0][4]);
    }

    #[test]
    fn the_max_factor_is_the_sweep_s_largest_magnitude_of_the_chosen_feed() {
        let req = InputImpedanceRequest {
            abs_seg: 6,
            norm: InputImpedanceNorm::Max,
        };
        let s = render_sweep_sections(&max_sweep(), Some(req), &[]);
        // nec2c: NORMALIZATION FACTOR 9.15419E+01; 28 MHz → 0.71131 − j0.51789.
        assert!(s.text.contains("NORMALIZATION_OHMS 91.54"), "{}", s.text);
        assert!(s.text.contains("NORMALIZED_TO MAX"));
        let r = rows(&s.text);
        assert!((r[0][5] - 0.71131).abs() < 1e-4 && (r[0][6] + 0.51789).abs() < 1e-4);
        assert!((r[2][7] - 1.0).abs() < 1e-12, "the largest point is 1");
        // The phase is not normalized: -36.06 degrees at 28 MHz.
        assert!((r[0][4] + 36.06).abs() < 0.01, "{}", r[0][4]);
    }

    #[test]
    fn the_table_follows_the_requested_feed_not_the_first() {
        let pts = vec![
            point(29.9, &[(10.0, 0.0), (153.12, 43.157)]),
            point(30.9, &[(20.0, 0.0), (160.0, 80.0)]),
        ];
        let req = InputImpedanceRequest {
            abs_seg: 17,
            norm: InputImpedanceNorm::Fixed(50.0),
        };
        let s = render_sweep_sections(&pts, Some(req), &[]);
        assert!(s
            .text
            .contains("SOURCE_TAG 2 SOURCE_SEG 6 SOURCE_SEGMENT 17\n"));
        assert_eq!(rows(&s.text)[0][1], 153.12);
        // SWEEP_POINTS keeps the first feed, as it always has.
        assert!(s.text.contains("29.900000 1 6 10.000000 0.000000\n"));
    }

    #[test]
    fn no_table_for_one_point_or_a_partial_sweep_or_no_request() {
        let req = InputImpedanceRequest {
            abs_seg: 6,
            norm: InputImpedanceNorm::Max,
        };
        // Success checks with each absence: the same call prints the table when
        // the condition is lifted.
        assert!(render_sweep_sections(&max_sweep(), Some(req), &[])
            .text
            .contains("INPUT_IMPEDANCE"));
        let one = render_sweep_sections(&max_sweep()[..1], Some(req), &[]);
        assert_eq!(one, SweepSections::default());
        let partial = render_sweep_sections(&max_sweep()[..2], Some(req), &[30.0]);
        assert!(partial.text.contains("SWEEP_POINTS\n"));
        assert!(!partial.text.contains("INPUT_IMPEDANCE"));
        assert!(
            partial.warnings.len() == 1
                && partial.warnings[0].contains("INPUT_IMPEDANCE")
                && partial.warnings[0].contains("30.000000 MHz failed"),
            "{:?}",
            partial.warnings
        );
        let none = render_sweep_sections(&max_sweep(), None, &[]);
        assert!(none.text.contains("SWEEP_POINTS\n") && !none.text.contains("INPUT_IMPEDANCE"));
    }

    /// The distributed route's shape: one feed per point, its segment resolved.
    #[test]
    fn a_one_feed_point_list_renders_both_sections() {
        let pts = vec![
            point(29.9, &[(78.614, 40.951)]),
            point(30.9, &[(86.740, 87.106)]),
        ];
        let req = InputImpedanceRequest {
            abs_seg: 6,
            norm: InputImpedanceNorm::Max,
        };
        let s = render_sweep_sections(&pts, Some(req), &[]);
        // Every number checked independently (Python's cmath on the same Z).
        assert_eq!(
            s.text,
            "\nSWEEP_POINTS\nN_POINTS 2\nFREQ_MHZ TAG SEG Z_RE Z_IM\n\
             29.900000 1 6 78.614000 40.951000\n30.900000 1 6 86.740000 87.106000\n\
             \nINPUT_IMPEDANCE\nSOURCE_TAG 1 SOURCE_SEG 6 SOURCE_SEGMENT 6\n\
             NORMALIZATION_OHMS 122.927958 NORMALIZED_TO MAX\nN_POINTS 2\n\
             FREQ_MHZ Z_RE Z_IM Z_ABS Z_PHASE_DEG ZN_RE ZN_IM ZN_ABS\n\
             29.900000 78.614000 40.951000 88.640540 27.515562 6.395128e-1 3.331301e-1 7.210771e-1\n\
             30.900000 86.740000 87.106000 122.927958 45.120625 7.056165e-1 7.085939e-1 1.000000e0\n"
        );
    }
}
