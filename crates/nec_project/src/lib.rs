//! Project-container and workflow metadata scope for future frontends.
//!
//! This crate is intentionally minimal today, but its planned responsibility is
//! narrower than "anything that is not the solver": it is the home for
//! Markdown-based project manifests, run metadata/history, and result-storage
//! conventions that let CLI/GUI/TUI workflows share one project model.
//!
//! FR-004 tracks Markdown-based project import/export as an explicit product
//! requirement. Until that lands, this crate serves as the documented scope
//! boundary for that work rather than an implicit placeholder.
//!
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
