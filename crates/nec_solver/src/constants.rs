// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The physical constants the solver uses: `nec_model`'s, re-exported so solver
//! code and its callers name them through the solver (FND-297 b). See
//! [`nec_model::constants`] for the values and why `EPS0` is derived.

pub use nec_model::constants::{C0, EPS0, ETA0, MU0};
