// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Editable deck document (GUI-CHK-007).
//!
//! [`ModelDoc`] is the in-memory model behind the visual wire editor. It splits
//! a parsed deck into an editable table of [`WireRow`]s (the `GW` cards) plus the
//! surrounding cards, which are preserved verbatim. Each `WireRow` holds its
//! fields as **strings** (what a text box edits) and validates them back to a
//! [`GwCard`] on demand, so a half-typed `-` or empty box is an editing state,
//! not a crash.
//!
//! The document tracks a `dirty` flag (set on any edit, cleared on save) and can
//! render itself back to deck text via [`crate::deck_write::write_deck`] for the
//! live 3-D preview and for saving.

use nec_model::card::{Card, GwCard};
use nec_model::deck::NecDeck;

use crate::deck_write::write_deck;

/// Which field of a [`WireRow`] an edit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireField {
    Tag,
    Segments,
    X1,
    Y1,
    Z1,
    X2,
    Y2,
    Z2,
    Radius,
}

/// One editable `GW` wire, fields as raw strings (text-box contents).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WireRow {
    pub tag: String,
    pub segments: String,
    pub x1: String,
    pub y1: String,
    pub z1: String,
    pub x2: String,
    pub y2: String,
    pub z2: String,
    pub radius: String,
}

/// Format an `f64` for a text box: shortest round-tripping decimal, integral
/// values without a trailing `.0`.
fn fmt_f(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

impl WireRow {
    /// Fill a row from a parsed `GW` card.
    pub fn from_card(c: &GwCard) -> Self {
        Self {
            tag: c.tag.to_string(),
            segments: c.segments.to_string(),
            x1: fmt_f(c.start[0]),
            y1: fmt_f(c.start[1]),
            z1: fmt_f(c.start[2]),
            x2: fmt_f(c.end[0]),
            y2: fmt_f(c.end[1]),
            z2: fmt_f(c.end[2]),
            radius: fmt_f(c.radius),
        }
    }

    /// A sensible blank wire for "Add row": 1 segment, unit length on x, thin.
    pub fn new_default(tag: u32) -> Self {
        Self::from_card(&GwCard {
            tag,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        })
    }

    /// Mutable access to a field by [`WireField`].
    pub fn field_mut(&mut self, field: WireField) -> &mut String {
        match field {
            WireField::Tag => &mut self.tag,
            WireField::Segments => &mut self.segments,
            WireField::X1 => &mut self.x1,
            WireField::Y1 => &mut self.y1,
            WireField::Z1 => &mut self.z1,
            WireField::X2 => &mut self.x2,
            WireField::Y2 => &mut self.y2,
            WireField::Z2 => &mut self.z2,
            WireField::Radius => &mut self.radius,
        }
    }

    /// Validate and convert to a [`GwCard`], or return a human-readable reason.
    pub fn to_card(&self) -> Result<GwCard, String> {
        let tag = parse_u(&self.tag, "tag")?;
        if tag < 1 {
            return Err("tag must be ≥ 1".into());
        }
        let segments = parse_u(&self.segments, "segments")?;
        if segments < 1 {
            return Err("segments must be ≥ 1".into());
        }
        let start = [
            parse_f(&self.x1, "x1")?,
            parse_f(&self.y1, "y1")?,
            parse_f(&self.z1, "z1")?,
        ];
        let end = [
            parse_f(&self.x2, "x2")?,
            parse_f(&self.y2, "y2")?,
            parse_f(&self.z2, "z2")?,
        ];
        let radius = parse_f(&self.radius, "radius")?;
        if radius <= 0.0 {
            return Err("radius must be > 0".into());
        }
        if start == end {
            return Err("start and end points are identical (zero-length wire)".into());
        }
        Ok(GwCard {
            tag,
            segments,
            start,
            end,
            radius,
        })
    }
}

fn parse_u(s: &str, name: &str) -> Result<u32, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(format!("{name} is empty"));
    }
    t.parse::<u32>()
        .map_err(|_| format!("{name}: '{t}' is not a whole number"))
}

fn parse_f(s: &str, name: &str) -> Result<f64, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(format!("{name} is empty"));
    }
    let v = t
        .parse::<f64>()
        .map_err(|_| format!("{name}: '{t}' is not a number"))?;
    if !v.is_finite() {
        return Err(format!("{name}: '{t}' is not finite"));
    }
    Ok(v)
}

/// An editable deck: the `GW` wires as a table, everything else preserved.
///
/// The non-wire cards are split around the first contiguous block of `GW` cards
/// into `pre` (leading comments, etc.) and `post` (`GE`, `EX`, `FR`, …). On
/// [`ModelDoc::to_deck`] they are re-emitted around the edited wire rows, so the
/// editor never drops the control cards it doesn't yet understand.
#[derive(Debug, Clone, Default)]
pub struct ModelDoc {
    pre: Vec<Card>,
    pub wires: Vec<WireRow>,
    post: Vec<Card>,
    pub dirty: bool,
}

impl ModelDoc {
    /// Build an editable document from a parsed deck.
    pub fn from_deck(deck: &NecDeck) -> Self {
        let mut pre = Vec::new();
        let mut wires = Vec::new();
        let mut post = Vec::new();
        let mut seen_wire = false;
        for card in &deck.cards {
            match card {
                Card::Gw(gw) if post.is_empty() => {
                    // Extend the (first) contiguous wire block.
                    wires.push(WireRow::from_card(gw));
                    seen_wire = true;
                }
                other if !seen_wire => pre.push(other.clone()),
                other => post.push(other.clone()),
            }
        }
        Self {
            pre,
            wires,
            post,
            dirty: false,
        }
    }

    /// Number of editable wires.
    pub fn wire_count(&self) -> usize {
        self.wires.len()
    }

    /// Edit a single field of a wire row and mark the document dirty.
    pub fn edit(&mut self, row: usize, field: WireField, value: String) {
        if let Some(w) = self.wires.get_mut(row) {
            *w.field_mut(field) = value;
            self.dirty = true;
        }
    }

    /// Append a new wire whose tag is one past the current maximum.
    pub fn add_wire(&mut self) {
        let next_tag = self
            .wires
            .iter()
            .filter_map(|w| w.tag.trim().parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        self.wires.push(WireRow::new_default(next_tag));
        self.dirty = true;
    }

    /// Remove a wire row by index (no-op if out of range).
    pub fn delete_wire(&mut self, row: usize) {
        if row < self.wires.len() {
            self.wires.remove(row);
            self.dirty = true;
        }
    }

    /// Validate every wire, returning the reconstructed deck or the first row
    /// that fails to validate (`(row_index, reason)`).
    pub fn to_deck(&self) -> Result<NecDeck, (usize, String)> {
        let mut cards = self.pre.clone();
        for (i, w) in self.wires.iter().enumerate() {
            let gw = w.to_card().map_err(|e| (i, e))?;
            cards.push(Card::Gw(gw));
        }
        cards.extend(self.post.iter().cloned());
        Ok(NecDeck { cards })
    }

    /// Render the current document to deck text, or the first validation error.
    pub fn to_deck_string(&self) -> Result<String, (usize, String)> {
        self.to_deck().map(|d| write_deck(&d))
    }

    /// Mark the document saved (clears `dirty`).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_parser::parse;

    const DIPOLE: &str = "\
CM test dipole
CE
GW 1 51 0 0 -5.232 0 0 5.232 0.001
GE 0
EX 0 1 26 0 1
FR 0 1 14.2 0
EN
";

    fn doc() -> ModelDoc {
        ModelDoc::from_deck(&parse(DIPOLE).unwrap().deck)
    }

    #[test]
    fn from_deck_splits_wires_and_preserves_rest() {
        let d = doc();
        assert_eq!(d.wire_count(), 1);
        assert_eq!(d.wires[0].tag, "1");
        assert_eq!(d.wires[0].z2, "5.232");
        assert!(!d.dirty);
        // pre = 2 comments; post = GE, EX, FR, EN.
        let rebuilt = d.to_deck().unwrap();
        // Round-trips exactly through the writer/parser.
        let reparsed = parse(&write_deck(&rebuilt)).unwrap().deck;
        assert_eq!(reparsed.cards, parse(DIPOLE).unwrap().deck.cards);
    }

    #[test]
    fn editing_a_coordinate_changes_the_deck_and_sets_dirty() {
        let mut d = doc();
        d.edit(0, WireField::Z2, "6.0".into());
        assert!(d.dirty);
        let deck = d.to_deck().unwrap();
        let Card::Gw(gw) = deck
            .cards
            .iter()
            .find(|c| matches!(c, Card::Gw(_)))
            .unwrap()
        else {
            unreachable!()
        };
        assert_eq!(gw.end[2], 6.0);
    }

    #[test]
    fn add_and_delete_wire() {
        let mut d = doc();
        d.add_wire();
        assert_eq!(d.wire_count(), 2);
        assert_eq!(d.wires[1].tag, "2", "new tag is max+1");
        d.delete_wire(0);
        assert_eq!(d.wire_count(), 1);
        assert_eq!(d.wires[0].tag, "2", "row 0 removed");
    }

    #[test]
    fn invalid_fields_are_rejected_with_row_and_reason() {
        let mut d = doc();
        d.edit(0, WireField::Radius, "0".into());
        let err = d.to_deck().unwrap_err();
        assert_eq!(err.0, 0);
        assert!(err.1.contains("radius"), "unexpected: {}", err.1);

        d.edit(0, WireField::Radius, "0.001".into());
        d.edit(0, WireField::Segments, "0".into());
        assert!(d.to_deck().unwrap_err().1.contains("segments"));

        d.edit(0, WireField::Segments, "51".into());
        d.edit(0, WireField::X1, "abc".into());
        assert!(d.to_deck().unwrap_err().1.contains("x1"));
    }

    #[test]
    fn zero_length_wire_is_rejected() {
        let mut d = doc();
        for (f, v) in [
            (WireField::X1, "0"),
            (WireField::Y1, "0"),
            (WireField::Z1, "0"),
            (WireField::X2, "0"),
            (WireField::Y2, "0"),
            (WireField::Z2, "0"),
        ] {
            d.edit(0, f, v.into());
        }
        assert!(d.to_deck().unwrap_err().1.contains("zero-length"));
    }

    #[test]
    fn empty_field_is_an_editing_state_not_a_panic() {
        let mut d = doc();
        d.edit(0, WireField::Z2, String::new());
        assert!(d.to_deck().is_err());
        // Typing resumes cleanly.
        d.edit(0, WireField::Z2, "5.232".into());
        assert!(d.to_deck().is_ok());
    }
}
