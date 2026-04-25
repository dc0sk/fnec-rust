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

#[derive(Debug, Clone, PartialEq)]
pub struct ReportInput<'a> {
    pub solver_mode: &'a str,
    pub pulse_rhs: &'a str,
    pub frequency_hz: f64,
    pub rows: &'a [FeedpointRow],
    /// Segment current distribution table.  When non-empty, appended after the
    /// feedpoint section as `CURRENTS / TAG SEG I_RE I_IM I_MAG I_PHASE` rows.
    pub current_table: &'a [CurrentRow],
}

pub fn render_text_report(input: &ReportInput<'_>) -> String {
    let mut out = String::new();

    out.push_str("FNEC FEEDPOINT REPORT\n");
    out.push_str("FORMAT_VERSION 1\n");
    out.push_str(&format!("FREQ_MHZ {:.6}\n", input.frequency_hz / 1e6));
    out.push_str(&format!("SOLVER_MODE {}\n", input.solver_mode));
    out.push_str(&format!("PULSE_RHS {}\n", input.pulse_rhs));
    out.push_str("\n");
    out.push_str("FEEDPOINTS\n");
    out.push_str("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n");

    for row in input.rows {
        out.push_str(&format_feedpoint_row(row));
        out.push('\n');
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
            current_table: &[],
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
            current_table: &current_table,
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
}
