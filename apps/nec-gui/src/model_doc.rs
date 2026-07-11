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

use nec_model::card::{Card, ExCard, FrCard, GnCard, GwCard, LdCard};
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

// ── Editable control cards (GUI-CHK-008) ──────────────────────────────────────

/// Editable `EX` excitation (voltage/current source view). The type and driven
/// segment are edited as strings; the plane-wave-only fields are preserved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExRow {
    pub kind: String,
    pub tag: String,
    pub segment: String,
    pub vr: String,
    pub vi: String,
    // Preserved verbatim (not exposed for source-type editing).
    i4: u32,
    pol_deg: f64,
    pol_ratio: f64,
    theta_inc: f64,
    phi_inc: f64,
}

impl ExRow {
    fn from_card(c: &ExCard) -> Self {
        Self {
            kind: c.excitation_type.to_string(),
            tag: c.tag.to_string(),
            segment: c.segment.to_string(),
            vr: fmt_f(c.voltage_real),
            vi: fmt_f(c.voltage_imag),
            i4: c.i4,
            pol_deg: c.polarization_deg,
            pol_ratio: c.polarization_ratio,
            theta_inc: c.theta_inc,
            phi_inc: c.phi_inc,
        }
    }

    /// A default 1 V voltage source on tag 1, segment 1.
    fn new_default() -> Self {
        Self::from_card(&ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 1,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        })
    }

    fn to_card(&self) -> Result<ExCard, String> {
        Ok(ExCard {
            excitation_type: parse_u(&self.kind, "EX type")?,
            tag: parse_u(&self.tag, "EX tag")?,
            segment: parse_u(&self.segment, "EX segment")?,
            i4: self.i4,
            voltage_real: parse_f(&self.vr, "EX voltage (real)")?,
            voltage_imag: parse_f(&self.vi, "EX voltage (imag)")?,
            polarization_deg: self.pol_deg,
            polarization_ratio: self.pol_ratio,
            theta_inc: self.theta_inc,
            phi_inc: self.phi_inc,
        })
    }
}

/// Editable `GN` ground. `ground_type` is a fixed choice (pick-list in the UI);
/// the medium fields are optional strings (empty = omitted).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GnRow {
    pub ground_type: i32,
    pub eps_r: String,
    pub sigma: String,
}

impl GnRow {
    fn from_card(c: &GnCard) -> Self {
        Self {
            ground_type: c.ground_type,
            eps_r: c.eps_r.map(fmt_f).unwrap_or_default(),
            sigma: c.sigma.map(fmt_f).unwrap_or_default(),
        }
    }

    /// A default perfect-conductor ground (`GN 1`, no medium).
    fn new_default() -> Self {
        Self {
            ground_type: 1,
            eps_r: String::new(),
            sigma: String::new(),
        }
    }

    fn to_card(&self) -> Result<GnCard, String> {
        Ok(GnCard {
            ground_type: self.ground_type,
            eps_r: parse_opt_f(&self.eps_r, "GN eps_r")?,
            sigma: parse_opt_f(&self.sigma, "GN sigma")?,
        })
    }
}

/// Editable `LD` load. `load_type` is a fixed choice (pick-list in the UI).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LdRow {
    pub load_type: i32,
    pub tag: String,
    pub seg_first: String,
    pub seg_last: String,
    pub f1: String,
    pub f2: String,
    pub f3: String,
}

impl LdRow {
    fn from_card(c: &LdCard) -> Self {
        Self {
            load_type: c.load_type,
            tag: c.tag.to_string(),
            seg_first: c.seg_first.to_string(),
            seg_last: c.seg_last.to_string(),
            f1: fmt_f(c.f1),
            f2: fmt_f(c.f2),
            f3: fmt_f(c.f3),
        }
    }

    /// A default series-impedance load (`LD 4`) of 0 Ω on tag 1, segment 1.
    fn new_default() -> Self {
        Self::from_card(&LdCard {
            load_type: 4,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 0.0,
            f2: 0.0,
            f3: 0.0,
        })
    }

    fn to_card(&self) -> Result<LdCard, String> {
        Ok(LdCard {
            load_type: self.load_type,
            tag: parse_u(&self.tag, "LD tag")?,
            seg_first: parse_u(&self.seg_first, "LD seg_first")?,
            seg_last: parse_u(&self.seg_last, "LD seg_last")?,
            f1: parse_f(&self.f1, "LD F1")?,
            f2: parse_f(&self.f2, "LD F2")?,
            f3: parse_f(&self.f3, "LD F3")?,
        })
    }
}

/// Editable `FR` frequency. `step_type` is a fixed choice (pick-list in the UI).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrRow {
    pub step_type: u32,
    pub steps: String,
    pub frequency_mhz: String,
    pub step_mhz: String,
}

impl FrRow {
    fn from_card(c: &FrCard) -> Self {
        Self {
            step_type: c.step_type,
            steps: c.steps.to_string(),
            frequency_mhz: fmt_f(c.frequency_mhz),
            step_mhz: fmt_f(c.step_mhz),
        }
    }

    fn to_card(&self) -> Result<FrCard, String> {
        let steps = parse_u(&self.steps, "FR steps")?;
        if steps < 1 {
            return Err("FR steps must be ≥ 1".into());
        }
        let frequency_mhz = parse_f(&self.frequency_mhz, "FR frequency")?;
        if frequency_mhz <= 0.0 {
            return Err("FR frequency must be > 0".into());
        }
        Ok(FrCard {
            step_type: self.step_type,
            steps,
            frequency_mhz,
            step_mhz: parse_f(&self.step_mhz, "FR step")?,
        })
    }
}

/// A card in the post-geometry section: either an editable control card or an
/// opaque preserved card (comments, `GE`, `GM`, `RP`, `NE`, `EN`, …).
#[derive(Debug, Clone, PartialEq)]
pub enum PostSlot {
    Ex(ExRow),
    Gn(GnRow),
    Ld(LdRow),
    Fr(FrRow),
    Other(Card),
}

impl PostSlot {
    fn from_card(card: Card) -> Self {
        match card {
            Card::Ex(c) => PostSlot::Ex(ExRow::from_card(&c)),
            Card::Gn(c) => PostSlot::Gn(GnRow::from_card(&c)),
            Card::Ld(c) => PostSlot::Ld(LdRow::from_card(&c)),
            Card::Fr(c) => PostSlot::Fr(FrRow::from_card(&c)),
            other => PostSlot::Other(other),
        }
    }

    fn to_card(&self) -> Result<Card, String> {
        Ok(match self {
            PostSlot::Ex(r) => Card::Ex(r.to_card()?),
            PostSlot::Gn(r) => Card::Gn(r.to_card()?),
            PostSlot::Ld(r) => Card::Ld(r.to_card()?),
            PostSlot::Fr(r) => Card::Fr(r.to_card()?),
            PostSlot::Other(c) => c.clone(),
        })
    }
}

/// A kind of control card that can be inserted into a deck that lacks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Gn,
    Ld,
    Ex,
}

impl PostSlot {
    /// NEC deck-ordering rank, used to insert a new control card in a sensible
    /// place: geometry/comments (0) < GN (1) < LD (2) < EX (3) < FR (4) <
    /// output/misc RP/NE/… (5) < EN (9).
    fn order_rank(&self) -> u8 {
        match self {
            PostSlot::Gn(_) => 1,
            PostSlot::Ld(_) => 2,
            PostSlot::Ex(_) => 3,
            PostSlot::Fr(_) => 4,
            PostSlot::Other(c) => match c {
                Card::Comment(_) | Card::Ge(_) | Card::Gm(_) | Card::Gr(_) => 0,
                Card::En(_) => 9,
                _ => 5,
            },
        }
    }
}

/// A single control-card field edit, carried by one message. Applied to the
/// addressed post slot; a kind mismatch (edit vs slot) is a no-op.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlEdit {
    ExKind(String),
    ExTag(String),
    ExSegment(String),
    ExVr(String),
    ExVi(String),
    GnType(i32),
    GnEps(String),
    GnSigma(String),
    LdType(i32),
    LdTag(String),
    LdSegFirst(String),
    LdSegLast(String),
    LdF1(String),
    LdF2(String),
    LdF3(String),
    FrStepType(u32),
    FrSteps(String),
    FrFrequency(String),
    FrStep(String),
}

impl ControlEdit {
    /// Short stable key for undo coalescing (same field = same run).
    pub fn field_key(&self) -> &'static str {
        match self {
            ControlEdit::ExKind(_) => "ex.kind",
            ControlEdit::ExTag(_) => "ex.tag",
            ControlEdit::ExSegment(_) => "ex.seg",
            ControlEdit::ExVr(_) => "ex.vr",
            ControlEdit::ExVi(_) => "ex.vi",
            ControlEdit::GnType(_) => "gn.type",
            ControlEdit::GnEps(_) => "gn.eps",
            ControlEdit::GnSigma(_) => "gn.sigma",
            ControlEdit::LdType(_) => "ld.type",
            ControlEdit::LdTag(_) => "ld.tag",
            ControlEdit::LdSegFirst(_) => "ld.segf",
            ControlEdit::LdSegLast(_) => "ld.segl",
            ControlEdit::LdF1(_) => "ld.f1",
            ControlEdit::LdF2(_) => "ld.f2",
            ControlEdit::LdF3(_) => "ld.f3",
            ControlEdit::FrStepType(_) => "fr.steptype",
            ControlEdit::FrSteps(_) => "fr.steps",
            ControlEdit::FrFrequency(_) => "fr.freq",
            ControlEdit::FrStep(_) => "fr.step",
        }
    }

    /// Apply this edit to a post slot, if the slot kind matches.
    fn apply_to(&self, slot: &mut PostSlot) {
        match (self, slot) {
            (ControlEdit::ExKind(v), PostSlot::Ex(r)) => r.kind = v.clone(),
            (ControlEdit::ExTag(v), PostSlot::Ex(r)) => r.tag = v.clone(),
            (ControlEdit::ExSegment(v), PostSlot::Ex(r)) => r.segment = v.clone(),
            (ControlEdit::ExVr(v), PostSlot::Ex(r)) => r.vr = v.clone(),
            (ControlEdit::ExVi(v), PostSlot::Ex(r)) => r.vi = v.clone(),
            (ControlEdit::GnType(v), PostSlot::Gn(r)) => r.ground_type = *v,
            (ControlEdit::GnEps(v), PostSlot::Gn(r)) => r.eps_r = v.clone(),
            (ControlEdit::GnSigma(v), PostSlot::Gn(r)) => r.sigma = v.clone(),
            (ControlEdit::LdType(v), PostSlot::Ld(r)) => r.load_type = *v,
            (ControlEdit::LdTag(v), PostSlot::Ld(r)) => r.tag = v.clone(),
            (ControlEdit::LdSegFirst(v), PostSlot::Ld(r)) => r.seg_first = v.clone(),
            (ControlEdit::LdSegLast(v), PostSlot::Ld(r)) => r.seg_last = v.clone(),
            (ControlEdit::LdF1(v), PostSlot::Ld(r)) => r.f1 = v.clone(),
            (ControlEdit::LdF2(v), PostSlot::Ld(r)) => r.f2 = v.clone(),
            (ControlEdit::LdF3(v), PostSlot::Ld(r)) => r.f3 = v.clone(),
            (ControlEdit::FrStepType(v), PostSlot::Fr(r)) => r.step_type = *v,
            (ControlEdit::FrSteps(v), PostSlot::Fr(r)) => r.steps = v.clone(),
            (ControlEdit::FrFrequency(v), PostSlot::Fr(r)) => r.frequency_mhz = v.clone(),
            (ControlEdit::FrStep(v), PostSlot::Fr(r)) => r.step_mhz = v.clone(),
            // Kind mismatch (stale message vs current slot) — ignore.
            _ => {}
        }
    }
}

fn parse_opt_f(s: &str, name: &str) -> Result<Option<f64>, String> {
    if s.trim().is_empty() {
        Ok(None)
    } else {
        parse_f(s, name).map(Some)
    }
}

/// An editable deck: the `GW` wires as a table, everything else preserved.
///
/// The non-wire cards are split around the first contiguous block of `GW` cards
/// into `pre` (leading comments, etc.) and `post` (`GE`, `GN`, `LD`, `EX`, `FR`,
/// …). Control cards in `post` become typed, editable [`PostSlot`]s; the rest are
/// preserved verbatim. On [`ModelDoc::to_deck`] everything is re-emitted around
/// the edited wire rows in original order.
#[derive(Debug, Clone, Default)]
pub struct ModelDoc {
    pre: Vec<Card>,
    pub wires: Vec<WireRow>,
    post: Vec<PostSlot>,
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
                other => post.push(PostSlot::from_card(other.clone())),
            }
        }
        Self {
            pre,
            wires,
            post,
            dirty: false,
        }
    }

    /// The post-geometry slots (control cards + preserved cards), for the editors.
    pub fn post_slots(&self) -> &[PostSlot] {
        &self.post
    }

    /// Apply a control-card edit to a post slot (no-op on a kind mismatch or an
    /// out-of-range index) and mark the document dirty.
    pub fn edit_control(&mut self, slot: usize, edit: &ControlEdit) {
        if let Some(s) = self.post.get_mut(slot) {
            edit.apply_to(s);
            self.dirty = true;
        }
    }

    /// Insert a new default control card (`GN`/`LD`/`EX`), placed in canonical
    /// NEC order among the existing post cards, and mark the document dirty.
    pub fn add_control(&mut self, kind: ControlKind) {
        let slot = match kind {
            ControlKind::Gn => PostSlot::Gn(GnRow::new_default()),
            ControlKind::Ld => PostSlot::Ld(LdRow::new_default()),
            ControlKind::Ex => PostSlot::Ex(ExRow::new_default()),
        };
        let rank = slot.order_rank();
        // Insert before the first existing card that should sort after the new one.
        let idx = self
            .post
            .iter()
            .position(|s| s.order_rank() > rank)
            .unwrap_or(self.post.len());
        self.post.insert(idx, slot);
        self.dirty = true;
    }

    /// Remove a post slot by index (no-op if out of range) and mark dirty.
    pub fn delete_control(&mut self, slot: usize) {
        if slot < self.post.len() {
            self.post.remove(slot);
            self.dirty = true;
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

    /// Validate every wire and control card, returning the reconstructed deck or
    /// the first field that fails to validate (message already prefixed with the
    /// offending row/card, e.g. `"wire 2: radius must be > 0"`).
    pub fn to_deck(&self) -> Result<NecDeck, String> {
        let mut cards = self.pre.clone();
        for (i, w) in self.wires.iter().enumerate() {
            let gw = w.to_card().map_err(|e| format!("wire {}: {e}", i + 1))?;
            cards.push(Card::Gw(gw));
        }
        for slot in &self.post {
            cards.push(slot.to_card()?);
        }
        Ok(NecDeck { cards })
    }

    /// Render the current document to deck text, or the first validation error.
    pub fn to_deck_string(&self) -> Result<String, String> {
        self.to_deck().map(|d| write_deck(&d))
    }

    /// Mark the document saved (clears `dirty`).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

/// Upper bound on retained undo/redo snapshots (guards against unbounded growth
/// during a long editing session).
const MAX_HISTORY: usize = 200;

/// Undo/redo history for the wire editor (GUI-CHK-007).
///
/// Snapshots are whole [`ModelDoc`] clones. Consecutive edits to the **same**
/// `(row, field)` are *coalesced*: only the pre-run state is snapshotted, so
/// typing `-5.232` into one box is a single undo step, not one step per keystroke.
/// Structural changes (add/delete) and any undo/redo break the run.
#[derive(Debug, Clone, Default)]
pub struct EditHistory {
    undo: Vec<ModelDoc>,
    redo: Vec<ModelDoc>,
    /// The field currently absorbing coalesced edits, if a run is open. Keyed by
    /// an opaque per-field string (e.g. `"wire:0:Z2"`, `"ex:1:vr"`), so any
    /// editable field — wire or control card — coalesces its own typing run.
    last_key: Option<String>,
}

impl EditHistory {
    /// Whether an undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Clear all history (called when a fresh deck is loaded into the editor).
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_key = None;
    }

    fn push_undo(&mut self, doc: &ModelDoc) {
        self.undo.push(doc.clone());
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Snapshot the pre-edit state before a field edit, coalescing consecutive
    /// edits carrying the same `key` into one undo step. Call *before* mutating
    /// the document. The key identifies the specific field being typed into
    /// (e.g. `"wire:0:Z2"`), so switching fields opens a fresh step.
    pub fn before_field_edit(&mut self, doc: &ModelDoc, key: &str) {
        if self.last_key.as_deref() == Some(key) {
            return;
        }
        self.push_undo(doc);
        self.last_key = Some(key.to_string());
    }

    /// Snapshot the pre-change state before a structural edit (add/delete row).
    /// Never coalesced. Call *before* mutating the document.
    pub fn before_structural(&mut self, doc: &ModelDoc) {
        self.push_undo(doc);
        self.last_key = None;
    }

    /// Restore the previous snapshot into `doc`, pushing the current state onto
    /// the redo stack. Returns `false` (and leaves `doc` untouched) if empty.
    pub fn undo(&mut self, doc: &mut ModelDoc) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(doc, prev));
                self.last_key = None;
                true
            }
            None => false,
        }
    }

    /// Re-apply the next redo snapshot into `doc`, pushing the current state onto
    /// the undo stack. Returns `false` (and leaves `doc` untouched) if empty.
    pub fn redo(&mut self, doc: &mut ModelDoc) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(doc, next));
                self.last_key = None;
                true
            }
            None => false,
        }
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
        assert!(err.contains("wire 1"), "should name the row: {err}");
        assert!(err.contains("radius"), "unexpected: {err}");

        d.edit(0, WireField::Radius, "0.001".into());
        d.edit(0, WireField::Segments, "0".into());
        assert!(d.to_deck().unwrap_err().contains("segments"));

        d.edit(0, WireField::Segments, "51".into());
        d.edit(0, WireField::X1, "abc".into());
        assert!(d.to_deck().unwrap_err().contains("x1"));
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
        assert!(d.to_deck().unwrap_err().contains("zero-length"));
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

    // ── EditHistory ──────────────────────────────────────────────────────────

    /// One field edited several times in a row is a single undo step, and undo
    /// restores the original value.
    #[test]
    fn history_coalesces_consecutive_same_field_edits() {
        let mut d = doc();
        let mut h = EditHistory::default();
        for v in ["5", "5.", "5.2", "5.23"] {
            h.before_field_edit(&d, "wire:0:Z2");
            d.edit(0, WireField::Z2, v.into());
        }
        assert_eq!(d.wires[0].z2, "5.23");
        assert!(h.can_undo());
        assert!(h.undo(&mut d), "one undo");
        assert_eq!(d.wires[0].z2, "5.232", "undo restores the pre-run value");
        assert!(!h.can_undo(), "the whole typing run was one step");
    }

    /// Editing a different field opens a new undo step.
    #[test]
    fn history_separates_different_fields() {
        let mut d = doc();
        let mut h = EditHistory::default();
        h.before_field_edit(&d, "wire:0:Z2");
        d.edit(0, WireField::Z2, "6".into());
        h.before_field_edit(&d, "wire:0:X1");
        d.edit(0, WireField::X1, "1".into());

        assert!(h.undo(&mut d)); // undoes X1
        assert_eq!(d.wires[0].x1, "0");
        assert_eq!(d.wires[0].z2, "6", "Z2 edit still applied");
        assert!(h.undo(&mut d)); // undoes Z2
        assert_eq!(d.wires[0].z2, "5.232");
    }

    /// Add/delete are their own undo steps and are not coalesced with edits.
    #[test]
    fn history_structural_changes_and_redo() {
        let mut d = doc();
        let mut h = EditHistory::default();
        h.before_structural(&d);
        d.add_wire();
        assert_eq!(d.wire_count(), 2);

        assert!(h.undo(&mut d));
        assert_eq!(d.wire_count(), 1, "undo removes the added wire");
        assert!(h.can_redo());
        assert!(h.redo(&mut d));
        assert_eq!(d.wire_count(), 2, "redo re-adds it");
    }

    /// A fresh edit after an undo clears the redo stack.
    #[test]
    fn history_new_edit_clears_redo() {
        let mut d = doc();
        let mut h = EditHistory::default();
        h.before_field_edit(&d, "wire:0:Z2");
        d.edit(0, WireField::Z2, "6".into());
        h.undo(&mut d);
        assert!(h.can_redo());
        h.before_field_edit(&d, "wire:0:Z2");
        d.edit(0, WireField::Z2, "7".into());
        assert!(!h.can_redo(), "a new edit invalidates redo");
    }

    #[test]
    fn history_undo_redo_on_empty_is_a_noop() {
        let mut d = doc();
        let mut h = EditHistory::default();
        assert!(!h.undo(&mut d));
        assert!(!h.redo(&mut d));
        assert_eq!(d.wire_count(), 1);
    }

    // ── Control-card slots (GUI-CHK-008) ─────────────────────────────────────

    const GROUND_DECK: &str = "\
CM ground dipole
CE
GW 1 21 0 0 5 0 0 15 0.001
GE 1
GN 2 0 0 0 13 0.005
LD 4 1 11 11 50 10 0
EX 0 1 11 0 1
FR 0 1 14.2 0
EN
";

    fn gnd_doc() -> ModelDoc {
        ModelDoc::from_deck(&parse(GROUND_DECK).unwrap().deck)
    }

    #[test]
    fn post_slots_are_typed_in_order() {
        let d = gnd_doc();
        let kinds: Vec<&str> = d
            .post_slots()
            .iter()
            .map(|s| match s {
                PostSlot::Gn(_) => "gn",
                PostSlot::Ld(_) => "ld",
                PostSlot::Ex(_) => "ex",
                PostSlot::Fr(_) => "fr",
                PostSlot::Other(_) => "other",
            })
            .collect();
        // GE(other), GN, LD, EX, FR, EN(other)
        assert_eq!(kinds, ["other", "gn", "ld", "ex", "fr", "other"]);
    }

    #[test]
    fn control_cards_round_trip_through_model_doc() {
        let d = gnd_doc();
        let rebuilt = d.to_deck().expect("valid");
        let reparsed = parse(&write_deck(&rebuilt)).unwrap().deck;
        assert_eq!(reparsed.cards, parse(GROUND_DECK).unwrap().deck.cards);
    }

    #[test]
    fn edit_control_changes_a_frequency_field() {
        let mut d = gnd_doc();
        // FR is slot 4.
        d.edit_control(4, &ControlEdit::FrFrequency("21.0".into()));
        assert!(d.dirty);
        let deck = d.to_deck().unwrap();
        let fr = deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Fr(f) => Some(f),
                _ => None,
            })
            .unwrap();
        assert_eq!(fr.frequency_mhz, 21.0);
    }

    #[test]
    fn edit_control_changes_ground_type_and_medium() {
        let mut d = gnd_doc();
        // GN is slot 1.
        d.edit_control(1, &ControlEdit::GnType(1));
        d.edit_control(1, &ControlEdit::GnEps(String::new()));
        d.edit_control(1, &ControlEdit::GnSigma(String::new()));
        let deck = d.to_deck().unwrap();
        let gn = deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Gn(g) => Some(g),
                _ => None,
            })
            .unwrap();
        assert_eq!(gn.ground_type, 1);
        assert_eq!(gn.eps_r, None, "cleared medium becomes None");
    }

    #[test]
    fn every_control_edit_variant_applies() {
        // gnd_doc post slots: GE(0), GN(1), LD(2), EX(3), FR(4), EN(5).
        let mut d = gnd_doc();
        // EX (slot 3).
        d.edit_control(3, &ControlEdit::ExKind("5".into()));
        d.edit_control(3, &ControlEdit::ExTag("2".into()));
        d.edit_control(3, &ControlEdit::ExSegment("7".into()));
        d.edit_control(3, &ControlEdit::ExVr("3".into()));
        d.edit_control(3, &ControlEdit::ExVi("4".into()));
        // GN (slot 1).
        d.edit_control(1, &ControlEdit::GnType(0));
        d.edit_control(1, &ControlEdit::GnEps("11".into()));
        d.edit_control(1, &ControlEdit::GnSigma("0.002".into()));
        // LD (slot 2).
        d.edit_control(2, &ControlEdit::LdType(1));
        d.edit_control(2, &ControlEdit::LdTag("3".into()));
        d.edit_control(2, &ControlEdit::LdSegFirst("4".into()));
        d.edit_control(2, &ControlEdit::LdSegLast("5".into()));
        d.edit_control(2, &ControlEdit::LdF1("6".into()));
        d.edit_control(2, &ControlEdit::LdF2("7".into()));
        d.edit_control(2, &ControlEdit::LdF3("8".into()));
        // FR (slot 4).
        d.edit_control(4, &ControlEdit::FrStepType(1));
        d.edit_control(4, &ControlEdit::FrSteps("9".into()));
        d.edit_control(4, &ControlEdit::FrFrequency("28.0".into()));
        d.edit_control(4, &ControlEdit::FrStep("0.1".into()));

        // Every field landed in its typed row.
        let slots = d.post_slots();
        let PostSlot::Ex(ex) = &slots[3] else {
            panic!()
        };
        assert_eq!(
            (
                ex.kind.as_str(),
                ex.tag.as_str(),
                ex.segment.as_str(),
                ex.vr.as_str(),
                ex.vi.as_str()
            ),
            ("5", "2", "7", "3", "4")
        );
        let PostSlot::Gn(gn) = &slots[1] else {
            panic!()
        };
        assert_eq!(
            (gn.ground_type, gn.eps_r.as_str(), gn.sigma.as_str()),
            (0, "11", "0.002")
        );
        let PostSlot::Ld(ld) = &slots[2] else {
            panic!()
        };
        assert_eq!(
            (
                ld.load_type,
                ld.tag.as_str(),
                ld.seg_first.as_str(),
                ld.seg_last.as_str(),
                ld.f1.as_str(),
                ld.f2.as_str(),
                ld.f3.as_str()
            ),
            (1, "3", "4", "5", "6", "7", "8")
        );
        let PostSlot::Fr(fr) = &slots[4] else {
            panic!()
        };
        assert_eq!(
            (
                fr.step_type,
                fr.steps.as_str(),
                fr.frequency_mhz.as_str(),
                fr.step_mhz.as_str()
            ),
            (1, "9", "28.0", "0.1")
        );
        // Still renders to a valid deck.
        assert!(d.to_deck().is_ok());
    }

    #[test]
    fn control_field_key_is_stable_per_field() {
        // Each variant has a distinct coalescing key (used for undo grouping).
        let keys = [
            ControlEdit::ExVr(String::new()).field_key(),
            ControlEdit::ExVi(String::new()).field_key(),
            ControlEdit::GnType(0).field_key(),
            ControlEdit::LdF1(String::new()).field_key(),
            ControlEdit::FrStep(String::new()).field_key(),
        ];
        // All distinct.
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b, "field keys must be distinct");
            }
        }
    }

    #[test]
    fn edit_control_kind_mismatch_is_a_noop() {
        let mut d = gnd_doc();
        // Slot 1 is GN; an LD edit must not touch it.
        let before = d.post_slots()[1].clone();
        d.edit_control(1, &ControlEdit::LdF1("999".into()));
        assert_eq!(d.post_slots()[1], before, "GN slot unchanged by an LD edit");
    }

    #[test]
    fn invalid_control_field_is_rejected() {
        let mut d = gnd_doc();
        d.edit_control(4, &ControlEdit::FrFrequency("abc".into()));
        let err = d.to_deck().unwrap_err();
        assert!(err.contains("FR frequency"), "unexpected: {err}");
    }

    /// Adding GN/LD/EX to a free-space deck inserts them in canonical NEC order
    /// (GN before LD before EX before FR) and the result still round-trips.
    #[test]
    fn add_control_inserts_in_canonical_order() {
        // Free-space dipole: post = [EX, FR, EN].
        let mut d = doc();
        d.add_control(ControlKind::Gn);
        d.add_control(ControlKind::Ld);
        assert!(d.dirty);
        let kinds: Vec<&str> = d
            .post_slots()
            .iter()
            .map(|s| match s {
                PostSlot::Gn(_) => "gn",
                PostSlot::Ld(_) => "ld",
                PostSlot::Ex(_) => "ex",
                PostSlot::Fr(_) => "fr",
                PostSlot::Other(_) => "other",
            })
            .collect();
        // GE stays first, GN and LD land before EX/FR, EN stays last.
        assert_eq!(kinds, ["other", "gn", "ld", "ex", "fr", "other"]);
        // The synthesised cards are valid and round-trip.
        let rebuilt = d.to_deck().expect("valid");
        let reparsed = parse(&write_deck(&rebuilt)).unwrap().deck;
        assert!(reparsed.cards.iter().any(|c| matches!(c, Card::Gn(_))));
        assert!(reparsed.cards.iter().any(|c| matches!(c, Card::Ld(_))));
    }

    #[test]
    fn add_then_delete_control_restores_post() {
        let mut d = doc();
        let before = d.post_slots().len();
        d.add_control(ControlKind::Ex);
        assert_eq!(d.post_slots().len(), before + 1);
        // Delete the newly added EX (it sorts to index 0 of post, before the
        // original EX/FR/EN in a free-space deck).
        let ex_idx = d
            .post_slots()
            .iter()
            .position(|s| matches!(s, PostSlot::Ex(_)))
            .unwrap();
        d.delete_control(ex_idx);
        assert_eq!(d.post_slots().len(), before);
    }
}
