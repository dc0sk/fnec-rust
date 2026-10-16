// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PAR-011: the parser's program-card records are what NEC's card echo prints.
//!
//! 4nec2 reads its pattern request back from the NEC-2D output's card echo
//! (`***** DATA CARD NO.  n   XX  I1..I4  F1..F6`): with the echo removed, it
//! loses the impedance and miscounts the pattern lines (the removal ablation in
//! `docs/par011-dropin-evidence-memo.md`). So before fnec can print that echo, the
//! fields must survive parsing. For every fixture 4nec2 wrote, the parsed records
//! equal, card for card, the echo its own kernel printed for the same `.inp`.
//!
//! Two kernel rules for `EN` are checked here too, since the NEC-2D writer must
//! reproduce them:
//! - the kernel reads the end of the file as `EN`, so a fixture whose `.inp` has
//!   no `EN` ends its echo with one extra card;
//! - an `EN`, whether read from the file or from its end, is echoed with the
//!   previous card's fields (`dip_sweep` writes `EN` after its `RP` and the echo
//!   repeats the `RP`'s). Every other card's missing fields echo as zero: in
//!   `dip_ld` the second `LD`'s absent F2 prints 0, not the first `LD`'s 1e-6.
//!
//! The records keep what each card says; the `EN` rule belongs to the writer.

use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/par011")
}

/// The echo lines of a kernel `.out`: (mnemonic, I1..I4, F1..F6).
fn echo(out: &str) -> Vec<(String, [i32; 4], [f64; 6])> {
    out.lines()
        .filter(|l| l.contains("***** DATA CARD NO."))
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // "*****" "DATA" "CARD" "NO." n XX I1..I4 F1..F6
            assert_eq!(f.len(), 16, "echo line: {l}");
            let mut ints = [0; 4];
            let mut floats = [0.0; 6];
            for i in 0..4 {
                ints[i] = f[6 + i].parse().expect("integer field");
            }
            for i in 0..6 {
                floats[i] = f[10 + i].parse().expect("real field");
            }
            (f[5].to_string(), ints, floats)
        })
        .collect()
}

/// The echo prints six significant digits (`1PE12.5`).
fn same_real(a: f64, b: f64) -> bool {
    (a - b).abs() <= 5e-6 * a.abs().max(b.abs()).max(1e-30)
}

#[test]
fn every_fixture_card_is_recorded_as_the_kernel_echoed_it() {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixture dir")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "inp").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    names.sort();
    assert_eq!(names.len(), 15, "fixtures: {names:?}");
    let mut refused = Vec::new();
    let mut failures = Vec::new();
    for name in &names {
        let inp = std::fs::read_to_string(fixtures().join(format!("{name}.inp"))).unwrap();
        let out = std::fs::read_to_string(fixtures().join(format!("{name}.out"))).unwrap();
        let deck = match nec_parser::parse(&inp) {
            Ok(r) => r.deck,
            Err(e) => {
                refused.push((name.clone(), e.to_string()));
                continue;
            }
        };
        let records: Vec<(String, [i32; 4], [f64; 6])> = deck
            .program_cards
            .iter()
            .map(|c| {
                let (i, f) = c
                    .fields
                    .unwrap_or_else(|| panic!("{name}: {c:?} unreadable"));
                (c.mnemonic.clone(), i, f)
            })
            .collect();
        let mut records = records;
        // A deck that does not end in EN: the kernel read the end of the file as
        // one.
        if records.last().is_none_or(|r| r.0 != "EN") {
            records.push(("EN".to_string(), [0; 4], [0.0; 6]));
        }
        // An EN is echoed with the previous card's fields.
        let n = records.len();
        assert!(n >= 2, "{name}: no card before EN");
        let prev = (records[n - 2].1, records[n - 2].2);
        (records[n - 1].1, records[n - 1].2) = prev;
        let echoed = echo(&out);
        // A deck the kernel stopped on (no RUN TIME line): its echo ends at the
        // card it failed on.
        let failed = !out.lines().any(|l| l.starts_with(" RUN TIME"));
        if failed {
            failures.push(name.clone());
            assert!(
                !echoed.is_empty() && echoed.len() < records.len(),
                "{name}: a failed run echoes a non-empty prefix: {echoed:?}"
            );
            records.truncate(echoed.len());
        }
        assert_eq!(
            records.len(),
            echoed.len(),
            "{name}: records {records:?}\nechoed {echoed:?}"
        );
        for (r, e) in records.iter().zip(&echoed) {
            assert_eq!((&r.0, r.1), (&e.0, e.1), "{name}");
            assert!(
                r.2.iter().zip(&e.2).all(|(a, b)| same_real(*a, *b)),
                "{name}: {} reals {:?} vs echoed {:?}",
                r.0,
                r.2,
                e.2
            );
        }
    }
    // Surface patches are refused by fnec; the deck never reaches a record.
    let refused_names: Vec<&str> = refused.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(refused_names, ["sp_patch"], "{refused:?}");
    // The source on a missing segment: the kernel's failure output.
    assert_eq!(failures, ["err_seg"]);
}
