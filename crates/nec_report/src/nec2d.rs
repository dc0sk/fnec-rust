// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The NEC-2D report layout (PAR-011): fnec's answer printed as NEC-2's Fortran
//! kernel prints its own, so that 4nec2 — which rejects any other layout — can
//! read it in place of its kernel's output.
//!
//! Every line goes through [`crate::fortran::write`] with the FORMAT statement
//! NEC-2 uses for it (`nec2dx.f`, cited by statement number), so the layout is the
//! Fortran's own and not a transcription. This module formats only: every number
//! comes from the caller, which takes it from fnec's solve (`PointReport`) and its
//! deck. What 4nec2 reads of the result was measured by ablation
//! (`docs/par011-dropin-evidence-memo.md`); the fixtures in
//! `apps/nec-cli/tests/fixtures/par011/` gate the layout.

use nec_model::constants::C0;
use num_complex::Complex64;

use crate::fortran::{write, Val};
use crate::PointReport;

/// One `GW` card's row in the structure table (FORMAT 43).
#[derive(Debug, Clone, PartialEq)]
pub struct WireRow {
    pub tag: u32,
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub radius: f64,
    pub segments: u32,
    /// The wire's first and last segment numbers.
    pub first: u32,
    pub last: u32,
}

/// A line of the structure table between the wire rows: a geometry card that
/// prints its own (FORMATs 44–47).
#[derive(Debug, Clone, PartialEq)]
pub enum StructureLine {
    Wire(WireRow),
    /// `GM`: the card's I1, I2 and its seven reals.
    Moved {
        i1: i64,
        i2: i64,
        reals: [f64; 7],
    },
    /// `GX`: the reflected axes (X, Y, Z) and the tag increment.
    Reflected {
        axes: [bool; 3],
        tag_increment: i64,
    },
    /// `GR`: the rotation count and the tag increment.
    Rotated {
        copies: i64,
        tag_increment: i64,
    },
    /// `GS`: the scale factor.
    Scaled(f64),
}

/// One segment's row in the segmentation table (FORMAT 55): NEC-2's own numbering
/// and connection data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentRow {
    pub centre: [f64; 3],
    pub length: f64,
    /// Elevation and azimuth of the segment's direction, degrees.
    pub alpha_deg: f64,
    pub beta_deg: f64,
    pub radius: f64,
    /// NEC-2's ICON1 and ICON2: the segment joined at each end, signed as NEC-2
    /// signs it; the segment's own number at a ground contact; 0 at a free end.
    pub icon1: i64,
    pub icon2: i64,
    pub tag: u32,
}

/// The geometry section: what NEC-2 prints before the first data card.
#[derive(Debug, Clone, PartialEq)]
pub struct Structure {
    /// The comment cards' text, columns 3 to 78 as NEC-2 reads them.
    pub comments: Vec<String>,
    pub lines: Vec<StructureLine>,
    /// The `GE` card's ground flag: 0 none, 1 a ground plane, -1 a ground plane
    /// with no image interpolation.
    pub ge_flag: i32,
    /// Segments in a symmetric cell and NEC-2's symmetry flag (`NP`, `IPSYM`).
    pub symmetric_cell: u32,
    pub symmetry_flag: i32,
    /// Junctions of three or more segment ends, NEC-2's signed segment lists.
    pub junctions: Vec<Vec<i64>>,
    pub segments: Vec<SegmentRow>,
}

/// One echoed data card: the mnemonic and its fields as NEC-2 read them.
#[derive(Debug, Clone, PartialEq)]
pub struct EchoCard {
    pub mnemonic: String,
    pub ints: [i32; 4],
    pub floats: [f64; 6],
}

/// One row of the impedance-loading table (NEC-2's `PRNT`): integers and reals
/// that are zero print blank.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadRow {
    pub tag: i64,
    pub from: i64,
    pub thru: i64,
    /// The absolute numbers of the segments the card loads, resolved by the
    /// solver's range rule (`nec_solver::deck_segment_in_range`): NEC-2's "loaded
    /// twice" note is decided on them, not on the card's tag arithmetic (FND-282).
    pub segments: Vec<u32>,
    /// Resistance, inductance, capacitance, impedance real and imaginary,
    /// conductivity.
    pub values: [f64; 6],
    /// NEC-2's type label, padded as it prints it (`" SERIES "`, `"  WIRE  "`).
    pub kind: &'static str,
}

/// What NEC-2 prints in its environment section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Environment {
    FreeSpace,
    PerfectGround,
    /// A finite ground: relative permittivity, conductivity, complex dielectric
    /// constant, and whether NEC-2 solves it by Sommerfeld integration (`GN 2`).
    FiniteGround {
        eps_r: f64,
        sigma: f64,
        complex: Complex64,
        sommerfeld: bool,
    },
}

/// The pattern request of an `RP` card, as NEC-2 decodes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternRequest {
    /// XNDA's X digit: 0 major/minor axes, 1 vertical/horizontal.
    pub axes: i32,
    /// D: 0 power gain, 1 directive gain.
    pub directive: bool,
    /// The average power gain and the solid angle it averaged over, in units of
    /// π steradians, when A asks for it.
    pub average: Option<(f64, f64)>,
}

/// One feedpoint row of the input-parameter table (FORMAT 62): the global segment
/// number and the quantities NEC-2 prints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedRow {
    pub tag: u32,
    pub segment: u32,
    pub voltage: Complex64,
    pub current: Complex64,
    pub power_w: f64,
}

/// One current row (FORMAT 165): NEC-2's numbering, the centre and length in
/// wavelengths, and the current.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentRow {
    pub segment: u32,
    pub tag: u32,
    pub centre_wl: [f64; 3],
    pub length_wl: f64,
    pub current: Complex64,
}

/// Everything one frequency's block prints that `PointReport` does not carry in
/// NEC-2's own terms.
#[derive(Debug, Clone, PartialEq)]
pub struct Point<'a> {
    pub report: &'a PointReport,
    pub loads: &'a [LoadRow],
    pub environment: Environment,
    /// The matrix fill and factor time, seconds (FORMAT 153).
    pub fill_s: f64,
    pub factor_s: f64,
    pub feeds: Vec<FeedRow>,
    pub currents: Vec<CurrentRow>,
    pub pattern: Option<PatternRequest>,
    /// Whether the deck asks for the near electric field (an `NE` card).
    pub near_field: bool,
}

/// A whole run: the structure, the echoed cards before execution, each
/// frequency's block, the cards echoed after it, and the run time.
#[derive(Debug, Clone, PartialEq)]
pub struct Run<'a> {
    pub structure: &'a Structure,
    /// The cards NEC-2 echoes before it executes (up to and including the card
    /// that triggers the solve).
    pub before: &'a [EchoCard],
    pub points: Vec<Point<'a>>,
    /// The cards echoed after the solve: the `EN`.
    pub after: &'a [EchoCard],
    /// Whether that `EN` is the end of the file read as one (the deck had none):
    /// the kernel then prints FORMAT 135's blank lines before echoing it, as it
    /// does before reading any card; an `EN` the file holds is echoed at once.
    pub en_from_end_of_file: bool,
    /// Lines fnec prints after the last echo, before `RUN TIME`: its banner and
    /// caveats. 4nec2 tolerates free text there (the insertion ablation).
    pub notes: &'a [String],
    pub run_time_s: f64,
}

fn i(v: impl Into<i64>) -> Val<'static> {
    Val::I(v.into())
}

fn f(v: f64) -> Val<'static> {
    Val::F(v)
}

/// The header and comments (FORMATs 126–129).
fn banner(out: &mut String, comments: &[String]) {
    out.push_str(&write("('1')", &[]));
    out.push_str(&write(
        "(///,33X,'*********************************************',//,36X,\
         'NUMERICAL ELECTROMAGNETICS CODE (NEC-2D)',//,33X,\
         '*********************************************')",
        &[],
    ));
    out.push_str(&write("(////,37X,'- - - - COMMENTS - - - -',//)", &[]));
    for c in comments {
        // FORMAT 129, the card read as A2,19A4: 76 columns, blank-padded.
        let text: String = c.chars().take(76).collect();
        out.push_str(&write("(25X,A)", &[Val::S(&format!("{text:<76}"))]));
    }
}

/// The structure specification and segmentation data (FORMATs 40–55 and
/// CONNECT's 50–60).
fn structure(out: &mut String, s: &Structure) {
    out.push_str(&write(
        "(////,33X,35H- - - STRUCTURE SPECIFICATION - - -,//,37X,28H\
         COORDINATES MUST BE INPUT IN,/,37X,29HMETERS OR BE SCALED TO METER\
         S,/,37X,31HBEFORE STRUCTURE INPUT IS ENDED,//)",
        &[],
    ));
    out.push_str(&write(
        "(2X,4HWIRE,79X,6HNO. OF,4X,5HFIRST,2X,4HLAST,5X,3HTAG,/,2X,\
         3HNO.,8X,2HX1,9X,2HY1,9X,2HZ1,10X,2HX2,9X,2HY2,9X,2HZ2,6X,6HRADIUS\
         ,3X,4HSEG.,5X,4HSEG.,3X,4HSEG.,5X,3HNO.)",
        &[],
    ));
    for line in &s.lines {
        match line {
            StructureLine::Wire(w) => out.push_str(&write(
                "(1X,I5,3F11.5,1X,4F11.5,2X,I5,4X,I5,1X,I5,3X,I5)",
                &[
                    i(w.tag),
                    f(w.start[0]),
                    f(w.start[1]),
                    f(w.start[2]),
                    f(w.end[0]),
                    f(w.end[1]),
                    f(w.end[2]),
                    f(w.radius),
                    i(w.segments),
                    i(w.first),
                    i(w.last),
                    i(w.tag),
                ],
            )),
            StructureLine::Moved { i1, i2, reals } => {
                let mut vals = vec![i(*i1), i(*i2)];
                vals.extend(reals.iter().map(|r| f(*r)));
                out.push_str(&write(
                    "(6X,49HTHE STRUCTURE HAS BEEN MOVED, MOVE DATA CARD IS -/6X\
                     ,I3,I5,7F10.5)",
                    &vals,
                ));
            }
            StructureLine::Reflected {
                axes,
                tag_increment,
            } => {
                let a = |on: bool, c: &'static str| Val::S(if on { c } else { " " });
                out.push_str(&write(
                    "(6X,34HSTRUCTURE REFLECTED ALONG THE AXES,3(1X,A1),22H.  TA\
                     GS INCREMENTED BY,I5)",
                    &[
                        a(axes[0], "X"),
                        a(axes[1], "Y"),
                        a(axes[2], "Z"),
                        i(*tag_increment),
                    ],
                ));
            }
            StructureLine::Rotated {
                copies,
                tag_increment,
            } => out.push_str(&write(
                "(6X,30HSTRUCTURE ROTATED ABOUT Z-AXIS,I3,30H TIMES.  LABELS\
                 \x20INCREMENTED BY,I5)",
                &[i(*copies), i(*tag_increment)],
            )),
            StructureLine::Scaled(k) => {
                out.push_str(&write("(6X,26HSTRUCTURE SCALED BY FACTOR,F10.5)", &[f(*k)]));
            }
        }
    }
    // CONNECT.
    if s.ge_flag != 0 {
        out.push_str(&write("(/,3X,23HGROUND PLANE SPECIFIED.)", &[]));
        if s.ge_flag > 0 {
            out.push_str(&write(
                "(/,3X,46HWHERE WIRE ENDS TOUCH GROUND, CURRENT WILL BE ,38H\
                 INTERPOLATED TO IMAGE IN GROUND PLANE.,/)",
                &[],
            ));
        }
    }
    let n = s.segments.len() as i64;
    out.push_str(&write(
        "(/,3X,20HTOTAL SEGMENTS USED=,I5,5X,12HNO. SEG. IN ,17HA SY\
         MMETRIC CELL=,I5,5X,14HSYMMETRY FLAG=,I3)",
        &[i(n), i(s.symmetric_cell), i(s.symmetry_flag)],
    ));
    let cells = if s.symmetric_cell > 0 {
        n / i64::from(s.symmetric_cell)
    } else {
        1
    };
    if cells != 1 {
        if s.symmetry_flag < 0 {
            out.push_str(&write(
                "(14H STRUCTURE HAS,I4,25H FOLD ROTATIONAL SYMMETRY,/)",
                &[i(cells)],
            ));
        } else if s.symmetry_flag > 0 {
            let planes = if cells == 8 { 3 } else { cells / 2 };
            out.push_str(&write(
                "(14H STRUCTURE HAS,I2,19H PLANES OF SYMMETRY,/)",
                &[i(planes)],
            ));
        }
    }
    out.push_str(&write(
        "(//,9X,27H- MULTIPLE WIRE JUNCTIONS -,/,1X,8HJUNCTION,4X,36\
         HSEGMENTS  (- FOR END 1, + FOR END 2))",
        &[],
    ));
    if s.junctions.is_empty() {
        out.push_str(&write("(2X,4HNONE)", &[]));
    }
    for (k, j) in s.junctions.iter().enumerate() {
        // FORMAT 51: twenty per line, continuation lines indented.
        for (row, chunk) in j.chunks(20).enumerate() {
            let mut vals: Vec<Val> = Vec::new();
            let lead = if row == 0 {
                vals.push(i(k as i64 + 1));
                "1X,I5,5X"
            } else {
                "11X"
            };
            vals.extend(chunk.iter().map(|v| i(*v)));
            out.push_str(&write(&format!("({lead},{}I5)", chunk.len()), &vals));
        }
    }
    out.push_str(&write(
        "(////33X,33H- - - - SEGMENTATION DATA - - - -,//,40X,21HCOO\
         RDINATES IN METERS,//,25X,50HI+ AND I- INDICATE THE SEGMENTS BEFOR\
         E AND AFTER I,//)",
        &[],
    ));
    out.push_str(&write(
        "(2X,4HSEG.,3X,26HCOORDINATES OF SEG. CENTER,5X,4HSEG.,5X,18\
         HORIENTATION ANGLES,4X,4HWIRE,4X,15HCONNECTION DATA,3X,3HTAG,/,2X,\
         3HNO.,7X,1HX,9X,1HY,9X,1HZ,7X,6HLENGTH,5X,5HALPHA,5X,4HBETA,6X,6HR\
         ADIUS,4X,2HI-,3X,1HI,4X,2HI+,4X,3HNO.)",
        &[],
    ));
    for (k, seg) in s.segments.iter().enumerate() {
        out.push_str(&write(
            "(1X,I5,4F10.5,1X,3F10.5,1X,3I5,2X,I5)",
            &[
                i(k as i64 + 1),
                f(seg.centre[0]),
                f(seg.centre[1]),
                f(seg.centre[2]),
                f(seg.length),
                f(seg.alpha_deg),
                f(seg.beta_deg),
                f(seg.radius),
                i(seg.icon1),
                i(k as i64 + 1),
                i(seg.icon2),
                i(seg.tag),
            ],
        ));
    }
}

/// The echo of one data card (FORMAT 137), numbered as NEC-2 numbers them.
fn echo(out: &mut String, n: usize, card: &EchoCard) {
    let mut vals = vec![
        i(n as i64),
        Val::S(&card.mnemonic),
        i(card.ints[0]),
        i(card.ints[1]),
        i(card.ints[2]),
        i(card.ints[3]),
    ];
    vals.extend(card.floats.iter().map(|v| f(*v)));
    out.push_str(&write(
        "(1X,'***** DATA CARD NO.',I3,3X,A2,1X,I3,3(1X,I5),6(1X,1P,E12.5))",
        &vals,
    ));
}

/// The cards echoed between the structure and the first frequency.
fn echo_block(out: &mut String, first: usize, cards: &[EchoCard]) {
    for (k, card) in cards.iter().enumerate() {
        echo(out, first + k, card);
    }
}

fn frequency(out: &mut String, p: &Point) {
    let freq_mhz = p.report.frequency_hz / 1e6;
    // The solver's own C0, so the wavelength printed is the one fnec solved with.
    out.push_str(&write(
        "(////,33X,'- - - - - - FREQUENCY - - - - - -',//,36X,\
         'FREQUENCY=',1P,E11.4,' MHZ',/,36X,'WAVELENGTH=',E11.4,' METERS')",
        &[f(freq_mhz), f(C0 / p.report.frequency_hz)],
    ));
    // FORMAT 196: NEC-2's interaction approximation (the KH card's range), which
    // fnec never uses; printed with NEC-2's default, as every kernel run prints it.
    out.push_str(&write(
        "(////,20X,55HAPPROXIMATE INTEGRATION EMPLOYED FOR SEGMENT\
         S MORE THAN,F8.3,18H WAVELENGTHS APART)",
        &[f(1.0)],
    ));
    out.push_str(&write(
        "(///,30X,' - - - STRUCTURE IMPEDANCE LOADING - - -')",
        &[],
    ));
    if p.loads.is_empty() {
        out.push_str(&write("(/ ,35X,'THIS STRUCTURE IS NOT LOADED')", &[]));
    } else {
        out.push_str(&write(
            "(//,7X,8HLOCATION,10X,10HRESISTANCE,3X,10HINDUCTANCE,2X,11H\
             CAPACITANCE,7X,16HIMPEDANCE (OHMS),5X,12HCONDUCTIVITY,4X,4HTYPE,/,\
             4X,4HITAG,10H FROM THRU,10X,4HOHMS,8X,6HHENRYS,7X,6HFARADS,8X,4HRE\
             AL,6X,9HIMAGINARY,4X,10HMHOS/METER)",
            &[],
        ));
        for row in p.loads {
            load(out, row);
        }
        // FORMAT 26: NEC-2 notes a segment loaded by more than one card.
        if loaded_twice(p.loads) {
            out.push_str(&write(
                "(/,10X,74HNOTE, SOME OF THE ABOVE SEGMENTS HAVE BEEN LOADED\
                 \x20TWICE - IMPEDANCES ADDED)",
                &[],
            ));
        }
    }
    out.push_str(&write("(///,34X,'- - - ANTENNA ENVIRONMENT - - -',/)", &[]));
    match p.environment {
        Environment::FreeSpace => out.push_str(&write("(  44X,'FREE SPACE')", &[])),
        Environment::PerfectGround => out.push_str(&write("(  42X,'PERFECT GROUND')", &[])),
        Environment::FiniteGround {
            eps_r,
            sigma,
            complex,
            sommerfeld,
        } => {
            out.push_str(&write(
                "(40X,A)",
                &[Val::S(if sommerfeld {
                    "FINITE GROUND.  SOMMERFELD SOLUTION"
                } else {
                    "FINITE GROUND.  REFLECTION COEFFICIENT APPROXIMATION"
                })],
            ));
            out.push_str(&write(
                "(40X,'RELATIVE DIELECTRIC CONST.=',F7.3,/,40X,'CONDUCTIV',\
                 'ITY=',1P,E10.3,' MHOS/METER',/,40X,'COMPLEX DIELECTRIC CONSTANT='\
                 ,2E12.5)",
                &[f(eps_r), f(sigma), f(complex.re), f(complex.im)],
            ));
        }
    }
    out.push_str(&write(
        "(///,32X,'- - - MATRIX TIMING - - -',//,24X,'FILL=',F9.3,\
         ' SEC.,  FACTOR=',F9.3,' SEC.')",
        &[f(p.fill_s), f(p.factor_s)],
    ));
    // Input parameters (FORMATs 63, 60, 62).
    out.push_str(&write(
        "(///,42X,36H- - - ANTENNA INPUT PARAMETERS - - -)",
        &[],
    ));
    out.push_str(&write(
        "(/,3X,3HTAG,3X,4HSEG.,4X,15HVOLTAGE (VOLTS),9X,14HCURRENT (\
         AMPS),9X,16HIMPEDANCE (OHMS),8X,17HADMITTANCE (MHOS),6X,5HPOWER,/,\
         3X,3HNO.,3X,3HNO.,4X,4HREAL,8X,5HIMAG.,3(7X,4HREAL,8X,5HIMAG.),5X,\
         7H(WATTS))",
        &[],
    ));
    for feed in &p.feeds {
        let z = feed.voltage / feed.current;
        let y = feed.current / feed.voltage;
        out.push_str(&write(
            "(2(1X,I5),1P,9E12.5)",
            &[
                i(feed.tag),
                i(feed.segment),
                f(feed.voltage.re),
                f(feed.voltage.im),
                f(feed.current.re),
                f(feed.current.im),
                f(z.re),
                f(z.im),
                f(y.re),
                f(y.im),
                f(feed.power_w),
            ],
        ));
    }
    // Currents (FORMATs 161, 162, 165).
    out.push_str(&write(
        "(///,29X,'- - - CURRENTS AND LOCATION - - -',//,33X,\
         'DISTANCES IN WAVELENGTHS')",
        &[],
    ));
    out.push_str(&write(
        "(  //,2X,'SEG.',2X,'TAG',4X,'COORD. OF SEG. CENTER',5X,\
         'SEG.',12X,'- - - CURRENT (AMPS) - - -',/,2X,'NO.',3X,'NO.',\
         5X,'X',8X,'Y',8X,'Z',6X,'LENGTH',5X,'REAL',8X,'IMAG.',7X,'MAG.',\
         8X,'PHASE')",
        &[],
    ));
    for c in &p.currents {
        out.push_str(&write(
            "(1X,2I5,3F9.4,F9.5,1X,1P,3E12.4,0P,F9.3)",
            &[
                i(c.segment),
                i(c.tag),
                f(c.centre_wl[0]),
                f(c.centre_wl[1]),
                f(c.centre_wl[2]),
                f(c.length_wl),
                f(c.current.re),
                f(c.current.im),
                f(c.current.norm()),
                f(c.current.arg().to_degrees()),
            ],
        ));
    }
    // Power budget (FORMAT 166).
    if let Some(budget) = p.report.power_budget {
        out.push_str(&write(
            "(///,40X,'- - - POWER BUDGET - - -',//,43X,'INPUT POWER   =\
             ',1P,E11.4,' WATTS',/ ,43X,'RADIATED POWER=',E11.4,' WATTS',\
             /,43X,'STRUCTURE LOSS=',E11.4,' WATTS',/,43X,'NETWORK LOSS  =',\
             E11.4,' WATTS',/,43X,'EFFICIENCY    =',0P,F7.2,' PERCENT')",
            &[
                f(budget.input_w),
                f(budget.radiated_w().unwrap_or(0.0)),
                f(budget.structure_loss_w),
                f(budget.network_loss_w.unwrap_or(0.0)),
                f(budget.efficiency_pct().unwrap_or(0.0)),
            ],
        ));
    }
    if let Some(request) = p.pattern {
        pattern(out, p.report, request);
    }
    if p.near_field {
        near_field(out, p.report);
    }
}

/// The near electric field (NFPAT: FORMATs 10 and 11).
fn near_field(out: &mut String, report: &PointReport) {
    out.push_str(&write(
        "(///,35X,32H- - - NEAR ELECTRIC FIELDS - - -,//,12X,14H-  L\
         OCATION  -,21X,8H-  EX  -,15X,8H-  EY  -,15X,8H-  EZ  -,/,8X,1HX,1\
         0X,1HY,10X,1HZ,10X,9HMAGNITUDE,3X,5HPHASE,6X,9HMAGNITUDE,3X,5HPHAS\
         E,6X,9HMAGNITUDE,3X,5HPHASE,/,6X,6HMETERS,5X,6HMETERS,5X,6HMETERS,\
         8X,7HVOLTS/M,3X,7HDEGREES,6X,7HVOLTS/M,3X,7HDEGREES,6X,7HVOLTS/M,3\
         X,7HDEGREES)",
        &[],
    ));
    for row in &report.near_field_table {
        let mut vals = vec![f(row.x), f(row.y), f(row.z)];
        for e in [row.ex, row.ey, row.ez] {
            vals.push(f(e.norm()));
            vals.push(f(e.arg().to_degrees()));
        }
        out.push_str(&write(
            "(2X,3(2X,F9.4),1X,3(3X,1P,E11.4,2X,0P,F7.2))",
            &vals,
        ));
    }
}

fn loaded_twice(loads: &[LoadRow]) -> bool {
    // A segment is loaded twice when two cards resolve to it.
    for (a, la) in loads.iter().enumerate() {
        for lb in &loads[a + 1..] {
            if la.segments.iter().any(|s| lb.segments.contains(s)) {
                return true;
            }
        }
    }
    false
}

/// One loading row through NEC-2's `PRNT`: zeros print blank.
fn load(out: &mut String, row: &LoadRow) {
    let ints = if row.tag == 0 && row.from == 0 && row.thru == 0 {
        [
            "  ALL".to_string(),
            "     ".to_string(),
            "     ".to_string(),
        ]
    } else {
        [row.tag, row.from, row.thru].map(|v| {
            if v == 0 {
                "     ".to_string()
            } else {
                write("(I5)", &[i(v)]).trim_end_matches('\n').to_string()
            }
        })
    };
    let reals = row.values.map(|v| {
        if v.abs() > 1e-30 {
            write("(1P,E13.4)", &[f(v)])
                .trim_end_matches('\n')
                .to_string()
        } else {
            " ".repeat(13)
        }
    });
    let mut vals: Vec<Val> = ints.iter().map(|s| Val::S(s.as_str())).collect();
    vals.extend(reals.iter().map(|s| Val::S(s.as_str())));
    vals.push(Val::S(row.kind));
    out.push_str(&write("(/,3X,3A,3X,6A,3X,A)", &vals));
}

/// The radiation pattern (RDPAT: FORMATs 38, 40, 42, 44).
fn pattern(out: &mut String, report: &PointReport, request: PatternRequest) {
    out.push_str(&write("(///,48X,30H- - - RADIATION PATTERNS - - -)", &[]));
    // NEC-2's two A6 words before "GAINS -" (its IGNTP).
    let (gain_kind, gains) = if request.directive {
        ("  DIRE", "CTIVE ")
    } else {
        ("    - ", "POWER ")
    };
    let (v, h) = if request.axes == 1 {
        (" VERT.", " HOR. ")
    } else {
        ("MAJOR ", "MINOR ")
    };
    out.push_str(&write(
        "(/,2X,14H- - ANGLES - -,7X,2A6,7HGAINS -,7X,24H- - - POLARI\
         ZATION - - -,4X,20H- - - E(THETA) - - -,4X,18H- - - E(PHI) - - -,\
         /,2X,5HTHETA,5X,3HPHI,7X,A6,2X,A6,3X,5HTOTAL,6X,5HAXIAL,5X,4HTILT,\
         3X,5HSENSE,2(5X,9HMAGNITUDE,4X,6HPHASE ),/,2(1X,7HDEGREES,1X),3(\
         6X,2HDB),8X,5HRATIO,5X,4HDEG.,8X,2(6X,7HVOLTS/M,4X,7HDEGREES))",
        &[Val::S(gain_kind), Val::S(gains), Val::S(v), Val::S(h)],
    ));
    for row in &report.pattern_table {
        let ((e_theta, e_phi), ell) = row
            .e_field
            .zip(row.ellipse)
            .expect("the NEC-2D writer is given CPU patterns only");
        let sense = match ell.sense {
            crate::Sense::None => "      ",
            crate::Sense::Linear => "LINEAR",
            crate::Sense::Right => "RIGHT ",
            crate::Sense::Left => "LEFT  ",
        };
        out.push_str(&write(
            "(1X,F7.2,F9.2,3X,3F8.2,F11.5,F9.2,2X,A6,2(1P,E15.5,0P,F9.2))",
            &[
                f(row.theta_deg),
                f(row.phi_deg),
                f(row.gain_theta_dbi),
                f(row.gain_phi_dbi),
                f(row.gain_total_dbi),
                f(ell.axial_ratio),
                f(ell.tilt_deg),
                Val::S(sense),
                f(e_theta.norm()),
                f(e_theta.arg().to_degrees()),
                f(e_phi.norm()),
                f(e_phi.arg().to_degrees()),
            ],
        ));
    }
    if let Some((gain, solid_over_pi)) = request.average {
        out.push_str(&write(
            "(//,3X,19HAVERAGE POWER GAIN=,1P,E12.5,7X, 31HSOLID ANGLE U\
             SED IN AVERAGING=(,0P,F7.4,16H)*PI STERADIANS.,//)",
            &[f(gain), f(solid_over_pi)],
        ));
    }
}

/// Print a whole run.
pub fn render(run: &Run) -> String {
    let mut out = String::new();
    banner(&mut out, &run.structure.comments);
    structure(&mut out, run.structure);
    // FORMAT 135 before the first echoed card.
    out.push_str(&write("(/////)", &[]));
    echo_block(&mut out, 1, run.before);
    for p in &run.points {
        frequency(&mut out, p);
    }
    if run.en_from_end_of_file {
        out.push_str(&write("(/////)", &[]));
    }
    echo_block(&mut out, 1 + run.before.len(), run.after);
    for note in run.notes {
        out.push_str(&write("(1X,A)", &[Val::S(note)]));
    }
    out.push_str(&write("(/,11H RUN TIME =,F10.3)", &[f(run.run_time_s)]));
    out
}

/// A run stopped before it finished: the structure and cards printed so far, and
/// fnec's reason in place of a result. No `RUN TIME` line follows, which is how
/// 4nec2 knows the kernel failed (the failure contract it reads; it then shows
/// the text).
pub fn render_failure(geometry: Option<&Structure>, echoed: &[EchoCard], reason: &str) -> String {
    let mut out = String::new();
    if let Some(s) = geometry {
        banner(&mut out, &s.comments);
        structure(&mut out, s);
        out.push_str(&write("(/////)", &[]));
        echo_block(&mut out, 1, echoed);
    }
    out.push_str(&write("(///,10X,A)", &[Val::S(reason)]));
    out
}
