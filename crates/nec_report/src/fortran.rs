// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! A Fortran FORMAT interpreter for the subset NEC-2's output statements use.
//!
//! The NEC-2D report is specified by the FORMAT statements of NEC-2's Fortran
//! source (`nec2dx.f`), and 4nec2 reads that layout, rejecting another (PAR-011).
//! So the writer passes those statements here verbatim rather than transcribing
//! their widths by hand: `write("(1X,I5,4F10.5)", &[..])` prints what the Fortran
//! WRITE printed.
//!
//! Supported: `nX`, `Iw`, `Fw.d`, `Ew.d`, `kP` (persisting, scaling F output by
//! `10^k` and E's mantissa), `A`/`Aw`, quoted and Hollerith (`nH`) literals, `/`,
//! repeat counts on descriptors and parenthesised groups. Each `/` ends a record;
//! the end of the format ends the last. A field too narrow for its value is
//! filled with `*`, as Fortran does. Format reversion is not supported: a
//! statement must have a descriptor for every value.

/// One value for a data descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val<'a> {
    I(i64),
    F(f64),
    S(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Skip(usize),
    Int(usize),
    Fixed(usize, usize),
    Exp(usize, usize),
    Scale(i32),
    Str(Option<usize>),
    Lit(String),
    Slash,
    Group(usize, Vec<Item>),
}

fn parse(fmt: &str) -> Vec<Item> {
    let chars: Vec<char> = fmt.trim().chars().collect();
    let mut pos = 0;
    // The outermost parentheses are the statement's own.
    let mut items = parse_list(&chars, &mut pos);
    if let [Item::Group(1, inner)] = items.as_mut_slice() {
        return std::mem::take(inner);
    }
    items
}

fn number(chars: &[char], pos: &mut usize) -> Option<usize> {
    let start = *pos;
    while *pos < chars.len() && chars[*pos].is_ascii_digit() {
        *pos += 1;
    }
    (*pos > start).then(|| {
        chars[start..*pos]
            .iter()
            .collect::<String>()
            .parse()
            .unwrap()
    })
}

fn parse_list(chars: &[char], pos: &mut usize) -> Vec<Item> {
    let mut items = Vec::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == ',' || c == ' ' || c == '\n' {
            *pos += 1;
            continue;
        }
        if c == ')' {
            *pos += 1;
            return items;
        }
        if c == '/' {
            items.push(Item::Slash);
            *pos += 1;
            continue;
        }
        if c == '\'' {
            *pos += 1;
            let mut s = String::new();
            while *pos < chars.len() {
                if chars[*pos] == '\'' {
                    if chars.get(*pos + 1) == Some(&'\'') {
                        s.push('\'');
                        *pos += 2;
                        continue;
                    }
                    *pos += 1;
                    break;
                }
                s.push(chars[*pos]);
                *pos += 1;
            }
            items.push(Item::Lit(s));
            continue;
        }
        // A sign belongs to a scale factor: `-1P`.
        let negative = c == '-';
        if negative {
            *pos += 1;
        }
        let count = number(chars, pos);
        let d = chars.get(*pos).map(char::to_ascii_uppercase);
        match d {
            Some('(') => {
                *pos += 1;
                let inner = parse_list(chars, pos);
                items.push(Item::Group(count.unwrap_or(1), inner));
            }
            Some('X') => {
                *pos += 1;
                items.push(Item::Skip(count.unwrap_or(1)));
            }
            Some('P') => {
                *pos += 1;
                let k = count.unwrap_or(0) as i32;
                items.push(Item::Scale(if negative { -k } else { k }));
            }
            Some('H') => {
                *pos += 1;
                let n = count.expect("nH needs a count");
                let s: String = chars[*pos..*pos + n].iter().collect();
                *pos += n;
                items.push(Item::Lit(s));
            }
            Some(kind @ ('I' | 'F' | 'E' | 'A')) => {
                *pos += 1;
                let w = number(chars, pos);
                let dd = if chars.get(*pos) == Some(&'.') {
                    *pos += 1;
                    number(chars, pos)
                } else {
                    None
                };
                let item = match kind {
                    'I' => Item::Int(w.expect("Iw")),
                    'F' => Item::Fixed(w.expect("Fw.d"), dd.expect("Fw.d")),
                    'E' => Item::Exp(w.expect("Ew.d"), dd.expect("Ew.d")),
                    _ => Item::Str(w),
                };
                for _ in 0..count.unwrap_or(1) {
                    items.push(item.clone());
                }
            }
            other => panic!("unsupported FORMAT item {other:?} at {pos} in {chars:?}"),
        }
    }
    items
}

/// `w` stars: a value that does not fit its field.
fn stars(w: usize) -> String {
    "*".repeat(w)
}

fn right(s: String, w: usize) -> String {
    if s.len() > w {
        stars(w)
    } else {
        format!("{s:>w$}")
    }
}

/// `Fw.d`, the value already scaled by `kP`.
pub fn fixed(v: f64, w: usize, d: usize) -> String {
    right(format!("{v:.d$}"), w)
}

/// `Ew.d` under scale factor `k` (NEC-2 uses `1P`: one digit before the point
/// and `d` after; `0P`: `0.` then `d` digits). Two exponent digits, or three
/// without the `E` past ±99, as Fortran prints them.
pub fn exp(v: f64, w: usize, d: usize, k: i32) -> String {
    let (mantissa, e) = if v == 0.0 {
        (0.0, 0)
    } else {
        // With scale k, the mantissa has k digits before the point.
        let digits = d as i32 + if k > 0 { 1 } else { 0 };
        let s = format!("{:.*e}", (digits - 1).max(0) as usize, v.abs());
        let (m, e) = s.split_once('e').unwrap();
        let m: f64 = m.parse().unwrap();
        let e: i32 = e.parse().unwrap();
        // m in [1, 10): shift to k digits before the point.
        let shift = k - 1;
        (m * 10f64.powi(shift), e - shift)
    };
    let decimals = if k > 0 {
        (d as i32 - k + 1).max(0) as usize
    } else {
        d
    };
    let mut body = if k <= 0 {
        format!("{:.decimals$}", mantissa)
    } else {
        format!("{mantissa:.decimals$}")
    };
    if k <= 0 && body.starts_with("0.") {
        // 0P prints `0.ddddE+xx`; keep the leading zero (G77 keeps it when the
        // field has room).
    }
    if body.is_empty() {
        body = "0".to_string();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let exponent = if e.abs() <= 99 {
        format!("E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    } else {
        format!("{}{:03}", if e < 0 { '-' } else { '+' }, e.abs())
    };
    right(format!("{sign}{body}{exponent}"), w)
}

/// Print `vals` through the FORMAT statement `fmt`. Every record ends with `\n`.
pub fn write(fmt: &str, vals: &[Val]) -> String {
    let items = parse(fmt);
    let mut out = String::new();
    let mut line = String::new();
    let mut k = 0;
    let mut next = 0;
    run(&items, vals, &mut next, &mut k, &mut line, &mut out);
    assert_eq!(
        next,
        vals.len(),
        "FORMAT {fmt}: {} of {} values printed",
        next,
        vals.len()
    );
    out.push_str(&line);
    out.push('\n');
    out
}

fn run(
    items: &[Item],
    vals: &[Val],
    next: &mut usize,
    k: &mut i32,
    line: &mut String,
    out: &mut String,
) {
    for item in items {
        match item {
            Item::Skip(n) => line.push_str(&" ".repeat(*n)),
            Item::Lit(s) => line.push_str(s),
            Item::Scale(p) => *k = *p,
            Item::Slash => {
                out.push_str(line);
                out.push('\n');
                line.clear();
            }
            Item::Group(n, inner) => {
                for _ in 0..*n {
                    run(inner, vals, next, k, line, out);
                }
            }
            Item::Int(w) => {
                let Some(Val::I(v)) = vals.get(*next) else {
                    panic!("I{w}: expected an integer, got {:?}", vals.get(*next));
                };
                *next += 1;
                line.push_str(&right(v.to_string(), *w));
            }
            Item::Fixed(w, d) => {
                let Some(Val::F(v)) = vals.get(*next) else {
                    panic!("F{w}.{d}: expected a real, got {:?}", vals.get(*next));
                };
                *next += 1;
                line.push_str(&fixed(v * 10f64.powi(*k), *w, *d));
            }
            Item::Exp(w, d) => {
                let Some(Val::F(v)) = vals.get(*next) else {
                    panic!("E{w}.{d}: expected a real, got {:?}", vals.get(*next));
                };
                *next += 1;
                line.push_str(&exp(*v, *w, *d, *k));
            }
            Item::Str(w) => {
                let Some(Val::S(s)) = vals.get(*next) else {
                    panic!("A: expected a string, got {:?}", vals.get(*next));
                };
                *next += 1;
                match w {
                    // Aw: the leftmost w characters, padded right.
                    Some(w) => line.push_str(&format!("{:<w$.w$}", s)),
                    None => line.push_str(s),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines lifted from the nec2dxs fixtures, printed through NEC-2's own
    /// FORMAT statements (`nec2dx.f`).
    #[test]
    fn nec2_formats_print_what_the_kernel_printed() {
        // 137: the card echo.
        assert_eq!(
            write(
                "(1X,'***** DATA CARD NO.',I3,3X,A2,1X,I3,3(1X,I5),6(1X,1P,E12.5))",
                &[
                    Val::I(3),
                    Val::S("RP"),
                    Val::I(0),
                    Val::I(37),
                    Val::I(73),
                    Val::I(1003),
                    Val::F(-180.0),
                    Val::F(0.0),
                    Val::F(5.0),
                    Val::F(5.0),
                    Val::F(0.0),
                    Val::F(0.0),
                ]
            ),
            " ***** DATA CARD NO.  3   RP   0    37    73  1003 -1.80000E+02  0.00000E+00  5.00000E+00  5.00000E+00  0.00000E+00  0.00000E+00\n"
        );
        // 55: a segmentation row.
        assert_eq!(
            write(
                "(1X,I5,4F10.5,1X,3F10.5,1X,3I5,2X,I5)",
                &[
                    Val::I(1),
                    Val::F(0.0),
                    Val::F(-0.21493),
                    Val::F(0.0),
                    Val::F(0.05373),
                    Val::F(0.0),
                    Val::F(90.0),
                    Val::F(0.0001),
                    Val::I(0),
                    Val::I(1),
                    Val::I(2),
                    Val::I(1),
                ]
            ),
            "     1   0.00000  -0.21493   0.00000   0.05373    0.00000  90.00000   0.00010     0    1    2      1\n"
        );
        // 62: an input-parameter row; a negative 1PE12.5 fills its field.
        assert_eq!(
            write(
                "(2(1X,I5),1P,9E12.5)",
                &[
                    Val::I(1),
                    Val::I(11),
                    Val::F(1.0),
                    Val::F(0.0),
                    Val::F(9.46615e-3),
                    Val::F(-5.49233e-3),
                    Val::F(79.0337),
                    Val::F(45.8559),
                    Val::F(9.46615e-3),
                    Val::F(-5.49233e-3),
                    Val::F(4.73308e-3),
                ]
            ),
            "     1    11 1.00000E+00 0.00000E+00 9.46615E-03-5.49233E-03 7.90337E+01 4.58559E+01 9.46615E-03-5.49233E-03 4.73308E-03\n"
        );
        // 165: a current row; 0P restores F after 1P.
        assert_eq!(
            write(
                "(1X,2I5,3F9.4,F9.5,1X,1P,3E12.4,0P,F9.3)",
                &[
                    Val::I(1),
                    Val::I(1),
                    Val::F(0.0),
                    Val::F(0.0),
                    Val::F(-0.2383),
                    Val::F(0.02383),
                    Val::F(8.3313e-4),
                    Val::F(-5.7227e-4),
                    Val::F(1.0107e-3),
                    Val::F(-34.485),
                ]
            ),
            "     1    1   0.0000   0.0000  -0.2383  0.02383   8.3313E-04 -5.7227E-04  1.0107E-03  -34.485\n"
        );
        // 201: the success marker, after a blank record.
        assert_eq!(
            write("(/,11H RUN TIME =,F10.3)", &[Val::F(0.02)]),
            "\n RUN TIME =     0.020\n"
        );
    }

    #[test]
    fn leading_slashes_are_blank_records() {
        assert_eq!(write("(///,3X,'A')", &[]), "\n\n\n   A\n");
        assert_eq!(write("('A',//,'B')", &[]), "A\n\nB\n");
    }

    #[test]
    fn fortran_edge_cases() {
        // A scale factor persists, and scales F output too.
        assert_eq!(
            write("(1P,E10.3,F6.2)", &[Val::F(5.8e7), Val::F(1.5)]),
            " 5.800E+07 15.00\n"
        );
        // Past ±99 the exponent takes three digits and drops the E.
        assert_eq!(exp(1.5e-120, 12, 5, 1), " 1.50000-120");
        // Rounding carries into the exponent.
        assert_eq!(exp(9.999996, 12, 5, 1), " 1.00000E+01");
        // Too wide for the field: stars.
        assert_eq!(write("(I3)", &[Val::I(12345)]), "***\n");
        assert_eq!(fixed(123456.0, 7, 2), "*******");
        // A string field cuts and pads.
        assert_eq!(write("(A6,'|')", &[Val::S("LINEAR")]), "LINEAR|\n");
        assert_eq!(write("(A6,'|')", &[Val::S("RIGHT")]), "RIGHT |\n");
    }
}
