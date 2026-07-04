//! Stable report-format helpers shared by frontends.
//!
//! The primary public surface is [`ReportInput`], which accepts already-solved
//! numerical results plus optional operator tables and radiation-pattern rows.
//! [`render_text_report`] renders those inputs into the versioned text contract
//! currently used by the CLI (`FORMAT_VERSION 1`).
//!
//! This crate intentionally owns report formatting only. It does not solve,
//! postprocess, or discover data on its own; callers provide structured rows so
//! future frontends can share the same report contract without duplicating the
//! formatting rules ad hoc.
//!
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use num_complex::Complex64;

/// One row in the segment current distribution table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentRow {
    pub tag: usize,
    pub seg: usize,
    pub current: Complex64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedpointRow {
    pub tag: usize,
    pub seg: usize,
    pub v_source: Complex64,
    pub current: Complex64,
    pub z_in: Complex64,
}

/// One row in the source-definition table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceRow {
    pub excitation_type: u32,
    pub tag: u32,
    pub seg: u32,
    pub i4: u32,
    pub voltage_real: f64,
    pub voltage_imag: f64,
}

/// One row in the load-definition table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadRow {
    pub load_type: i32,
    pub tag: u32,
    pub seg_first: u32,
    pub seg_last: u32,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
}

/// One row in the radiation-pattern table (one (θ, φ) point).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternRow {
    /// Zenith angle θ in degrees (0 = +z axis).
    pub theta_deg: f64,
    /// Azimuth angle φ in degrees.
    pub phi_deg: f64,
    /// Total directivity (dBi).
    pub gain_total_dbi: f64,
    /// Theta-polarised (vertical) component directivity (dBi).
    pub gain_theta_dbi: f64,
    /// Phi-polarised (horizontal) component directivity (dBi).
    pub gain_phi_dbi: f64,
    /// Axial ratio |E_θ| / |E_φ|.
    pub axial_ratio: f64,
}

/// One row of an incident-plane-wave receive pattern (PH9-CHK-001): the antenna's
/// normalized response as a function of the wave's arrival direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReceivePatternRow {
    /// Incidence zenith angle θ in degrees.
    pub theta_deg: f64,
    /// Incidence azimuth angle φ in degrees.
    pub phi_deg: f64,
    /// Normalized receive response in dB (0 dB at the sweep's peak). Derived from
    /// the peak induced current, which tracks the transmit gain pattern by
    /// reciprocity.
    pub response_db: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportInput<'a> {
    pub solver_mode: &'a str,
    pub pulse_rhs: &'a str,
    pub frequency_hz: f64,
    pub rows: &'a [FeedpointRow],
    /// Source-definition table captured from EX cards.
    pub source_table: &'a [SourceRow],
    /// Load-definition table captured from LD cards.
    pub load_table: &'a [LoadRow],
    /// Segment current distribution table.  When non-empty, appended after the
    /// feedpoint section as `CURRENTS / TAG SEG I_RE I_IM I_MAG I_PHASE` rows.
    pub current_table: &'a [CurrentRow],
    /// Radiation-pattern table.  When non-empty, appended after the currents
    /// section as `RADIATION_PATTERN / THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO` rows.
    pub pattern_table: &'a [PatternRow],
    /// Incident-plane-wave receive pattern.  When non-empty, appended as
    /// `RECEIVE_PATTERN / THETA PHI RESPONSE_DB` rows (PH9-CHK-001).
    pub receive_pattern_table: &'a [ReceivePatternRow],
}

/// **Extension point EP-2 — feedpoint result filter.**
///
/// Implementors receive the slice of [`FeedpointRow`] values computed for a
/// single frequency point and return a filtered or transformed
/// `Vec<FeedpointRow>`.  The hook runs after solve and before report
/// rendering, so it cannot affect solver behaviour.
///
/// # Safety model
///
/// Implementations are plain in-process Rust.  All values passed through
/// `rows` are plain numeric data; no network, filesystem, or FFI access is
/// reachable through this interface.
///
/// # Example
///
/// ```
/// use nec_report::{ResultFilter, FeedpointRow};
/// use num_complex::Complex64;
///
/// struct DropHighImpedance { threshold_ohms: f64 }
///
/// impl ResultFilter for DropHighImpedance {
///     fn filter(&self, rows: &[FeedpointRow]) -> Vec<FeedpointRow> {
///         rows.iter()
///             .filter(|r| r.z_in.re.abs() < self.threshold_ohms)
///             .copied()
///             .collect()
///     }
/// }
///
/// let row = FeedpointRow {
///     tag: 1, seg: 1,
///     v_source: Complex64::new(1.0, 0.0),
///     current: Complex64::new(0.01, 0.0),
///     z_in: Complex64::new(50.0, 0.0),
/// };
/// let f = DropHighImpedance { threshold_ohms: 200.0 };
/// assert_eq!(f.filter(&[row]).len(), 1);
///
/// let big = FeedpointRow { z_in: Complex64::new(300.0, 0.0), ..row };
/// assert_eq!(f.filter(&[big]).len(), 0);
/// ```
pub trait ResultFilter {
    /// Returns a filtered or transformed copy of `rows`.  The original
    /// report pipeline is unaffected; callers may discard the result.
    fn filter(&self, rows: &[FeedpointRow]) -> Vec<FeedpointRow>;
}

/// **Extension point EP-3 — custom report section.**
///
/// Implementors produce an arbitrary text block that is appended verbatim
/// after the standard sections in the report output.  The section name and
/// content are entirely under the implementor's control.
///
/// # Safety model
///
/// Implementations are plain in-process Rust.  The trait carries no handles
/// to network sockets, file descriptors, or FFI pointers.  The report
/// pipeline calls `render()` once per section after all standard sections
/// have been built; it cannot influence solver behaviour.
///
/// # Example — summary statistics section
///
/// ```
/// use nec_report::{ReportSection, FeedpointRow, ReportInput, render_text_report_with_sections};
/// use num_complex::Complex64;
///
/// /// Appends a one-line |Z| summary.
/// struct ImpedanceSummary<'a> { rows: &'a [FeedpointRow] }
///
/// impl ReportSection for ImpedanceSummary<'_> {
///     fn render(&self) -> String {
///         let mut out = String::from("IMPEDANCE_SUMMARY\n");
///         for r in self.rows {
///             let mag = (r.z_in.re * r.z_in.re + r.z_in.im * r.z_in.im).sqrt();
///             out.push_str(&format!("TAG {} SEG {} |Z|={:.3} Ω\n", r.tag, r.seg, mag));
///         }
///         out
///     }
/// }
///
/// let row = FeedpointRow {
///     tag: 1, seg: 26,
///     v_source: Complex64::new(1.0, 0.0),
///     current: Complex64::new(0.013471, -0.002522),
///     z_in: Complex64::new(74.242874, 13.899516),
/// };
/// let input = ReportInput {
///     solver_mode: "hallen",
///     pulse_rhs: "Nec2",
///     frequency_hz: 14_200_000.0,
///     rows: &[row],
///     source_table: &[],
///     load_table: &[],
///     current_table: &[],
///     pattern_table: &[],
///     receive_pattern_table: &[],
/// };
/// let section = ImpedanceSummary { rows: &[row] };
/// let report = render_text_report_with_sections(&input, &[&section]);
/// assert!(report.contains("IMPEDANCE_SUMMARY\n"));
/// assert!(report.contains("|Z|=75."));
/// ```
pub trait ReportSection {
    /// Renders the custom section to a string.  The returned text is appended
    /// after all standard sections.  Trailing newlines are the implementor's
    /// responsibility.
    fn render(&self) -> String;
}

/// Renders a text report and appends zero or more custom sections from EP-3
/// implementors.
///
/// If `extra_sections` is empty this behaves identically to
/// [`render_text_report`].
///
/// # Example
///
/// ```
/// use nec_report::{ReportSection, FeedpointRow, ReportInput, render_text_report_with_sections};
/// use num_complex::Complex64;
///
/// struct Banner;
/// impl ReportSection for Banner {
///     fn render(&self) -> String { "MY_SECTION\nhello world\n".to_string() }
/// }
///
/// let row = FeedpointRow {
///     tag: 1, seg: 1,
///     v_source: Complex64::new(1.0, 0.0),
///     current: Complex64::new(0.02, 0.0),
///     z_in: Complex64::new(50.0, 0.0),
/// };
/// let input = ReportInput {
///     solver_mode: "hallen", pulse_rhs: "Nec2",
///     frequency_hz: 14e6,
///     rows: &[row],
///     source_table: &[], load_table: &[],
///     current_table: &[], pattern_table: &[], receive_pattern_table: &[],
/// };
/// let report = render_text_report_with_sections(&input, &[&Banner]);
/// assert!(report.contains("MY_SECTION\nhello world\n"));
/// ```
pub fn render_text_report_with_sections(
    input: &ReportInput<'_>,
    extra_sections: &[&dyn ReportSection],
) -> String {
    let mut out = render_text_report(input);
    for section in extra_sections {
        out.push('\n');
        out.push_str(&section.render());
    }
    out
}

pub fn render_text_report(input: &ReportInput<'_>) -> String {
    let mut out = String::new();

    out.push_str("FNEC FEEDPOINT REPORT\n");
    out.push_str("FORMAT_VERSION 1\n");
    out.push_str(&format!("FREQ_MHZ {:.6}\n", input.frequency_hz / 1e6));
    out.push_str(&format!("SOLVER_MODE {}\n", input.solver_mode));
    out.push_str(&format!("PULSE_RHS {}\n", input.pulse_rhs));
    out.push('\n');
    out.push_str("FEEDPOINTS\n");
    out.push_str("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n");

    for row in input.rows {
        out.push_str(&format_feedpoint_row(row));
        out.push('\n');
    }

    if !input.source_table.is_empty() {
        out.push('\n');
        out.push_str("SOURCES\n");
        out.push_str(&format!("N_SOURCES {}\n", input.source_table.len()));
        out.push_str("TYPE TAG SEG I4 V_RE V_IM\n");
        for row in input.source_table {
            out.push_str(&format_source_row(row));
            out.push('\n');
        }
    }

    if !input.load_table.is_empty() {
        out.push('\n');
        out.push_str("LOADS\n");
        out.push_str(&format!("N_LOADS {}\n", input.load_table.len()));
        out.push_str("TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3\n");
        for row in input.load_table {
            out.push_str(&format_load_row(row));
            out.push('\n');
        }
    }

    if !input.current_table.is_empty() {
        out.push('\n');
        out.push_str("CURRENTS\n");
        out.push_str("TAG SEG I_RE I_IM I_MAG I_PHASE\n");
        for row in input.current_table {
            out.push_str(&format_current_row(row));
            out.push('\n');
        }
    }

    if !input.pattern_table.is_empty() {
        out.push('\n');
        out.push_str("RADIATION_PATTERN\n");
        out.push_str(&format!("N_POINTS {}\n", input.pattern_table.len()));
        out.push_str("THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO\n");
        for row in input.pattern_table {
            out.push_str(&format_pattern_row(row));
            out.push('\n');
        }
    }

    if !input.receive_pattern_table.is_empty() {
        out.push('\n');
        out.push_str("RECEIVE_PATTERN\n");
        out.push_str(&format!("N_POINTS {}\n", input.receive_pattern_table.len()));
        out.push_str("THETA PHI RESPONSE_DB\n");
        for row in input.receive_pattern_table {
            out.push_str(&format!(
                "{:.4} {:.4} {:.4}\n",
                row.theta_deg, row.phi_deg, row.response_db
            ));
        }
    }

    out
}

pub fn format_feedpoint_row(row: &FeedpointRow) -> String {
    format!(
        "{} {} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6}",
        row.tag,
        row.seg,
        row.v_source.re,
        row.v_source.im,
        row.current.re,
        row.current.im,
        row.z_in.re,
        row.z_in.im,
    )
}

pub fn format_current_row(row: &CurrentRow) -> String {
    let mag = row.current.norm();
    let phase = row.current.arg().to_degrees();
    format!(
        "{} {} {:.6e} {:.6e} {:.6e} {:.4}",
        row.tag, row.seg, row.current.re, row.current.im, mag, phase
    )
}

pub fn format_source_row(row: &SourceRow) -> String {
    format!(
        "{} {} {} {} {:.6} {:.6}",
        row.excitation_type, row.tag, row.seg, row.i4, row.voltage_real, row.voltage_imag,
    )
}

pub fn format_load_row(row: &LoadRow) -> String {
    format!(
        "{} {} {} {} {:.6} {:.6} {:.6}",
        row.load_type, row.tag, row.seg_first, row.seg_last, row.f1, row.f2, row.f3,
    )
}

pub fn format_pattern_row(row: &PatternRow) -> String {
    format!(
        "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
        row.theta_deg,
        row.phi_deg,
        row.gain_total_dbi,
        row.gain_theta_dbi,
        row.gain_phi_dbi,
        row.axial_ratio,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_feedpoint_row_is_stable() {
        let row = FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(0.1234567, -0.7654321),
            current: Complex64::new(1.0, -2.0),
            z_in: Complex64::new(74.242874, 13.899516),
        };

        let line = format_feedpoint_row(&row);
        assert_eq!(
            line,
            "1 26 0.123457 -0.765432 1.000000 -2.000000 74.242874 13.899516"
        );
    }

    #[test]
    fn report_has_contract_headers_and_table() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.013471, -0.002522),
            z_in: Complex64::new(74.242874, 13.899516),
        }];
        let report = render_text_report(&ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows: &rows,
            source_table: &[],
            load_table: &[],
            current_table: &[],
            pattern_table: &[],
            receive_pattern_table: &[],
        });

        assert!(report.starts_with("FNEC FEEDPOINT REPORT\nFORMAT_VERSION 1\n"));
        assert!(report.contains("FREQ_MHZ 14.200000\n"));
        assert!(report.contains("SOLVER_MODE hallen\n"));
        assert!(report.contains("PULSE_RHS Nec2\n"));
        assert!(report.contains("FEEDPOINTS\n"));
        assert!(report.contains("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n"));
        assert!(report.contains("1 26 1.000000 0.000000 0.013471 -0.002522 74.242874 13.899516\n"));
        // No CURRENTS section when current_table is empty.
        assert!(!report.contains("CURRENTS\n"));
    }

    #[test]
    fn report_includes_currents_section_when_provided() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.013471, -0.002522),
            z_in: Complex64::new(74.242874, 13.899516),
        }];
        let current_table = [
            CurrentRow {
                tag: 1,
                seg: 1,
                current: Complex64::new(0.0, 0.0),
            },
            CurrentRow {
                tag: 1,
                seg: 26,
                current: Complex64::new(0.013471, -0.002522),
            },
        ];
        let report = render_text_report(&ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows: &rows,
            source_table: &[],
            load_table: &[],
            current_table: &current_table,
            pattern_table: &[],
            receive_pattern_table: &[],
        });

        assert!(report.contains("CURRENTS\n"));
        assert!(report.contains("TAG SEG I_RE I_IM I_MAG I_PHASE\n"));
        // Tip segment: magnitude should be effectively zero.
        let lines: Vec<&str> = report.lines().collect();
        let curr_lines: Vec<&&str> = lines
            .iter()
            .skip_while(|l| **l != "TAG SEG I_RE I_IM I_MAG I_PHASE")
            .skip(1)
            .collect();
        assert!(
            curr_lines.len() >= 2,
            "expected at least 2 current rows, got {}",
            curr_lines.len()
        );
    }

    #[test]
    fn format_current_row_is_stable() {
        let row = CurrentRow {
            tag: 1,
            seg: 26,
            current: Complex64::new(0.013471, -0.002522),
        };
        let line = format_current_row(&row);
        // Verify the line has 6 whitespace-separated columns.
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(
            cols.len(),
            6,
            "current row should have 6 columns, got: {line}"
        );
        assert_eq!(cols[0], "1");
        assert_eq!(cols[1], "26");
        // I_MAG should be positive.
        let mag: f64 = cols[4].parse().unwrap();
        assert!(mag > 0.0);
    }

    #[test]
    fn pattern_row_format_is_stable() {
        let row = PatternRow {
            theta_deg: 90.0,
            phi_deg: 0.0,
            gain_total_dbi: 2.1428,
            gain_theta_dbi: 2.1428,
            gain_phi_dbi: -999.99,
            axial_ratio: 0.0,
        };
        let line = format_pattern_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 6, "pattern row should have 6 columns: {line}");
        assert_eq!(cols[0], "90.0000");
        assert_eq!(cols[1], "0.0000");
    }

    #[test]
    fn report_includes_radiation_pattern_section_when_provided() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.013471, -0.002522),
            z_in: Complex64::new(74.242874, 13.899516),
        }];
        let pattern = [PatternRow {
            theta_deg: 90.0,
            phi_deg: 0.0,
            gain_total_dbi: 2.14,
            gain_theta_dbi: 2.14,
            gain_phi_dbi: -999.99,
            axial_ratio: 0.0,
        }];
        let report = render_text_report(&ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows: &rows,
            source_table: &[],
            load_table: &[],
            current_table: &[],
            pattern_table: &pattern,
            receive_pattern_table: &[],
        });
        assert!(report.contains("RADIATION_PATTERN\n"));
        assert!(report.contains("N_POINTS 1\n"));
        assert!(report.contains("THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO\n"));
    }

    #[test]
    fn report_includes_sources_and_loads_in_stable_section_order() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.013471, -0.002522),
            z_in: Complex64::new(74.242874, 13.899516),
        }];
        let source_table = [SourceRow {
            excitation_type: 0,
            tag: 1,
            seg: 26,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }];
        let load_table = [LoadRow {
            load_type: 2,
            tag: 1,
            seg_first: 26,
            seg_last: 26,
            f1: 5.0,
            f2: 1e-6,
            f3: 0.0,
        }];
        let report = render_text_report(&ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows: &rows,
            source_table: &source_table,
            load_table: &load_table,
            current_table: &[],
            pattern_table: &[],
            receive_pattern_table: &[],
        });

        let feed_idx = report.find("FEEDPOINTS\n").expect("missing FEEDPOINTS");
        let source_idx = report.find("SOURCES\n").expect("missing SOURCES");
        let load_idx = report.find("LOADS\n").expect("missing LOADS");
        assert!(feed_idx < source_idx);
        assert!(source_idx < load_idx);
        assert!(report.contains("N_SOURCES 1\n"));
        assert!(report.contains("TYPE TAG SEG I4 V_RE V_IM\n"));
        assert!(report.contains("N_LOADS 1\n"));
        assert!(report.contains("TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3\n"));
    }

    // ── EP-3 ReportSection tests ────────────────────────────────────────

    struct FixedSection(&'static str);
    impl ReportSection for FixedSection {
        fn render(&self) -> String {
            self.0.to_string()
        }
    }

    fn minimal_input<'a>(rows: &'a [FeedpointRow]) -> ReportInput<'a> {
        ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows,
            source_table: &[],
            load_table: &[],
            current_table: &[],
            pattern_table: &[],
            receive_pattern_table: &[],
        }
    }

    #[test]
    fn render_with_sections_no_extra_is_identical_to_base() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 26,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.013471, -0.002522),
            z_in: Complex64::new(74.242874, 13.899516),
        }];
        let input = minimal_input(&rows);
        assert_eq!(
            render_text_report(&input),
            render_text_report_with_sections(&input, &[])
        );
    }

    #[test]
    fn render_with_sections_appends_custom_section() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 1,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.02, 0.0),
            z_in: Complex64::new(50.0, 0.0),
        }];
        let input = minimal_input(&rows);
        let section = FixedSection("MY_SECTION\nsome data\n");
        let report = render_text_report_with_sections(&input, &[&section]);
        assert!(report.contains("MY_SECTION\nsome data\n"));
        // Standard headers still present.
        assert!(report.contains("FEEDPOINTS\n"));
    }

    #[test]
    fn render_with_sections_multiple_sections_appended_in_order() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 1,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.02, 0.0),
            z_in: Complex64::new(50.0, 0.0),
        }];
        let input = minimal_input(&rows);
        let s1 = FixedSection("SECTION_A\n");
        let s2 = FixedSection("SECTION_B\n");
        let report = render_text_report_with_sections(&input, &[&s1, &s2]);
        let a_pos = report.find("SECTION_A\n").expect("SECTION_A missing");
        let b_pos = report.find("SECTION_B\n").expect("SECTION_B missing");
        assert!(a_pos < b_pos, "sections should appear in order");
    }

    #[test]
    fn ep3_summary_statistics_section_renders_impedance() {
        let rows = [
            FeedpointRow {
                tag: 1,
                seg: 1,
                v_source: Complex64::new(1.0, 0.0),
                current: Complex64::new(0.02, 0.0),
                z_in: Complex64::new(50.0, 0.0),
            },
            FeedpointRow {
                tag: 1,
                seg: 26,
                v_source: Complex64::new(1.0, 0.0),
                current: Complex64::new(0.013471, -0.002522),
                z_in: Complex64::new(74.242874, 13.899516),
            },
        ];

        struct PeakImpedanceSection<'a>(&'a [FeedpointRow]);
        impl ReportSection for PeakImpedanceSection<'_> {
            fn render(&self) -> String {
                let peak = self
                    .0
                    .iter()
                    .map(|r| (r.z_in.re * r.z_in.re + r.z_in.im * r.z_in.im).sqrt())
                    .fold(f64::NEG_INFINITY, f64::max);
                format!("PEAK_IMPEDANCE\n|Z|_max={:.3}\n", peak)
            }
        }

        let input = minimal_input(&rows);
        let section = PeakImpedanceSection(&rows);
        let report = render_text_report_with_sections(&input, &[&section]);
        assert!(report.contains("PEAK_IMPEDANCE\n"));
        // Peak |Z| ≈ sqrt(74.24² + 13.90²) ≈ 75.22 Ω
        assert!(report.contains("|Z|_max=75."));
    }

    // ── Corner-case tests (BL-IMPR-004) ─────────────────────────────────

    /// Empty feedpoint slice: FEEDPOINTS header + column header must still
    /// appear; no data rows should follow.
    #[test]
    fn report_with_empty_feedpoint_rows_renders_headers_only() {
        let report = render_text_report(&ReportInput {
            solver_mode: "hallen",
            pulse_rhs: "Nec2",
            frequency_hz: 14_200_000.0,
            rows: &[],
            source_table: &[],
            load_table: &[],
            current_table: &[],
            pattern_table: &[],
            receive_pattern_table: &[],
        });
        assert!(report.contains("FEEDPOINTS\n"));
        assert!(report.contains("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n"));
        // No numeric data row should follow the column header.
        let after_header = report
            .split("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n")
            .nth(1)
            .unwrap_or("");
        let first_line = after_header.lines().next().unwrap_or("");
        assert!(
            first_line.is_empty(),
            "expected no data row after empty feedpoints, got: {first_line:?}"
        );
    }

    /// Empty pattern table: RADIATION_PATTERN section must be omitted.
    #[test]
    fn report_omits_radiation_pattern_section_when_empty() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 1,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.02, 0.0),
            z_in: Complex64::new(50.0, 0.0),
        }];
        let report = render_text_report(&minimal_input(&rows));
        assert!(!report.contains("RADIATION_PATTERN\n"));
    }

    /// NaN in feedpoint z_in: the formatter must not panic; the output line
    /// should still have 8 columns with the NaN tokens in the Z columns.
    #[test]
    fn format_feedpoint_row_survives_nan_z_in() {
        let row = FeedpointRow {
            tag: 1,
            seg: 1,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.02, 0.0),
            z_in: Complex64::new(f64::NAN, f64::NAN),
        };
        let line = format_feedpoint_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 8, "must produce 8 columns even for NaN: {line}");
    }

    /// NaN in pattern gain field: formatter must not panic and must produce 6
    /// columns.
    #[test]
    fn format_pattern_row_survives_nan_gain() {
        let row = PatternRow {
            theta_deg: 90.0,
            phi_deg: 0.0,
            gain_total_dbi: f64::NAN,
            gain_theta_dbi: f64::NAN,
            gain_phi_dbi: f64::NEG_INFINITY,
            axial_ratio: 0.0,
        };
        let line = format_pattern_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 6, "must produce 6 columns even for NaN: {line}");
    }

    /// Very large impedance values (wide columns): formatter should not panic
    /// and the Z columns should contain the large magnitude.
    #[test]
    fn format_feedpoint_row_handles_very_large_z() {
        let row = FeedpointRow {
            tag: 999,
            seg: 9999,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(1e-12, 0.0),
            z_in: Complex64::new(1e12, -1e12),
        };
        let line = format_feedpoint_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 8, "must produce 8 columns for large Z: {line}");
        // Z_RE column (index 6) should contain the large magnitude.
        let z_re: f64 = cols[6].parse().expect("Z_RE must be parseable");
        assert!((z_re - 1e12).abs() < 1.0, "Z_RE={z_re} expected ~1e12");
    }

    /// Very large gain values in pattern row: formatter must handle them.
    #[test]
    fn format_pattern_row_handles_very_large_gain() {
        let row = PatternRow {
            theta_deg: 0.0,
            phi_deg: 0.0,
            gain_total_dbi: 1e6,
            gain_theta_dbi: 1e6,
            gain_phi_dbi: -1e6,
            axial_ratio: 1e9,
        };
        let line = format_pattern_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(
            cols.len(),
            6,
            "must produce 6 columns for large gain: {line}"
        );
    }

    /// Pattern rows at the poles (θ=0 and θ=180): formatter must not panic.
    #[test]
    fn format_pattern_row_at_poles() {
        for theta in [0.0_f64, 180.0_f64] {
            let row = PatternRow {
                theta_deg: theta,
                phi_deg: 0.0,
                gain_total_dbi: 2.15,
                gain_theta_dbi: 2.15,
                gain_phi_dbi: -999.99,
                axial_ratio: 0.0,
            };
            let line = format_pattern_row(&row);
            let cols: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(
                cols.len(),
                6,
                "must produce 6 columns at theta={theta}: {line}"
            );
            let t: f64 = cols[0].parse().unwrap();
            assert!((t - theta).abs() < 0.001, "theta round-trip failed: {t}");
        }
    }

    /// `format_source_row` stability: 6 columns, correct field order.
    #[test]
    fn format_source_row_is_stable() {
        let row = SourceRow {
            excitation_type: 0,
            tag: 1,
            seg: 26,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: -0.5,
        };
        let line = format_source_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 6, "source row must have 6 columns: {line}");
        assert_eq!(cols[0], "0"); // excitation_type
        assert_eq!(cols[1], "1"); // tag
        assert_eq!(cols[2], "26"); // seg
        assert_eq!(cols[3], "0"); // i4
    }

    /// `format_load_row` stability: 7 columns, correct field order.
    #[test]
    fn format_load_row_is_stable() {
        let row = LoadRow {
            load_type: 2,
            tag: 1,
            seg_first: 10,
            seg_last: 20,
            f1: 5.0,
            f2: 1e-6,
            f3: 0.0,
        };
        let line = format_load_row(&row);
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols.len(), 7, "load row must have 7 columns: {line}");
        assert_eq!(cols[0], "2"); // load_type
        assert_eq!(cols[2], "10"); // seg_first
        assert_eq!(cols[3], "20"); // seg_last
    }

    // ── EP-2 ResultFilter tests ──────────────────────────────────────────

    struct IdentityFilter;
    impl ResultFilter for IdentityFilter {
        fn filter(&self, rows: &[FeedpointRow]) -> Vec<FeedpointRow> {
            rows.to_vec()
        }
    }

    struct DropAll;
    impl ResultFilter for DropAll {
        fn filter(&self, _rows: &[FeedpointRow]) -> Vec<FeedpointRow> {
            vec![]
        }
    }

    struct ThresholdFilter {
        max_re_ohms: f64,
    }
    impl ResultFilter for ThresholdFilter {
        fn filter(&self, rows: &[FeedpointRow]) -> Vec<FeedpointRow> {
            rows.iter()
                .filter(|r| r.z_in.re.abs() <= self.max_re_ohms)
                .copied()
                .collect()
        }
    }

    fn two_feedpoint_rows() -> [FeedpointRow; 2] {
        [
            FeedpointRow {
                tag: 1,
                seg: 1,
                v_source: Complex64::new(1.0, 0.0),
                current: Complex64::new(0.02, 0.0),
                z_in: Complex64::new(50.0, 0.0),
            },
            FeedpointRow {
                tag: 1,
                seg: 26,
                v_source: Complex64::new(1.0, 0.0),
                current: Complex64::new(0.013471, -0.002522),
                z_in: Complex64::new(300.0, 0.0),
            },
        ]
    }

    #[test]
    fn result_filter_identity_returns_all_rows() {
        let rows = two_feedpoint_rows();
        let filtered = IdentityFilter.filter(&rows);
        assert_eq!(filtered.len(), rows.len());
        for (a, b) in rows.iter().zip(filtered.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn result_filter_drop_all_returns_empty() {
        let rows = two_feedpoint_rows();
        let filtered = DropAll.filter(&rows);
        assert!(filtered.is_empty());
    }

    #[test]
    fn result_filter_threshold_passes_only_matching_rows() {
        let rows = two_feedpoint_rows();
        let f = ThresholdFilter { max_re_ohms: 100.0 };
        let filtered = f.filter(&rows);
        assert_eq!(filtered.len(), 1, "only the 50 Ω row should pass");
        assert!((filtered[0].z_in.re - 50.0).abs() < 1e-9);
    }

    #[test]
    fn result_filter_on_empty_slice_returns_empty() {
        let filtered = IdentityFilter.filter(&[]);
        assert!(filtered.is_empty());
        let filtered = ThresholdFilter { max_re_ohms: 1.0 }.filter(&[]);
        assert!(filtered.is_empty());
    }

    #[test]
    fn result_filter_threshold_with_nan_z_in_does_not_panic() {
        let rows = [FeedpointRow {
            tag: 1,
            seg: 1,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(0.02, 0.0),
            z_in: Complex64::new(f64::NAN, 0.0),
        }];
        // NaN.abs() is NaN, which is not <= any finite threshold, so it
        // should be silently dropped rather than panic.
        let f = ThresholdFilter { max_re_ohms: 100.0 };
        let filtered = f.filter(&rows);
        assert!(
            filtered.is_empty(),
            "NaN z_in should be dropped by threshold filter"
        );
    }
}
