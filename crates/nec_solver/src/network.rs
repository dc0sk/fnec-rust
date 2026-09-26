// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `TL` and `NT` two-port networks, connected across segment gaps (FND-123).
//!
//! NEC-2 connects a network's ports across the gaps of its port segments, in
//! parallel with whatever else is there. The port voltage is the gap voltage; a
//! gap with no source must carry zero net current into the junction of the wire
//! and the network (KCL); at a driven port the source is in parallel with the
//! network, and the source current is the segment current PLUS the network
//! branch. That is what nec2c prints as the input current, and what the input
//! impedance and power are computed from.
//!
//! fnec used to stamp these as series Z-parameters into the dimensionless Hallén
//! matrix, which was inert: +0.37 Ω where nec2c moves the same feed by +78.5 Ω.
//! A two-port couples port VOLTAGES, which are not unknowns of the Hallén system,
//! so no stamp can represent it.
//!
//! The solve is by superposition, as NEC-2's own network routine does it: the
//! structure's response to the real excitation, plus its response to a unit gap
//! at each undriven port, weighted by port voltages from a small linear system.
//! That is exact because the Hallén solve is linear in its right-hand side.
//!
//! `NT I1 I2 I3 I4 Y11r Y11i Y12r Y12i Y22r Y22i` gives the short-circuit
//! admittance parameters directly (reciprocal, `Y21 = Y12`). Several networks on
//! one port add; a network whose two ports are the same segment is a one-port of
//! `Y11 + Y22 + 2·Y12`, which the accumulation in [`Networks::admittance`]
//! produces by itself.

use num_complex::Complex64;

use nec_model::card::Card;
use nec_model::deck::NecDeck;

use crate::geometry::Segment;
use crate::tl::find_segment_index;

const ZERO: Complex64 = Complex64::new(0.0, 0.0);

/// One two-port between segments `seg_a` and `seg_b` (which may coincide).
#[derive(Debug, Clone, PartialEq)]
pub struct TwoPort {
    /// Global index of the port-1 segment.
    pub seg_a: usize,
    /// Global index of the port-2 segment.
    pub seg_b: usize,
    /// Short-circuit input admittance at port 1 (S).
    pub y11: Complex64,
    /// Transfer admittance, `Y12 = Y21` (S).
    pub y12: Complex64,
    /// Short-circuit input admittance at port 2 (S).
    pub y22: Complex64,
}

/// Every network in a deck, at one frequency.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Networks {
    /// The two-ports, in card order.
    pub two_ports: Vec<TwoPort>,
}

impl Networks {
    /// Whether the deck has no network at all.
    pub fn is_empty(&self) -> bool {
        self.two_ports.is_empty()
    }

    /// The distinct port segments, ascending. One voltage per segment, however
    /// many networks share it.
    pub fn ports(&self) -> Vec<usize> {
        let mut p: Vec<usize> = self
            .two_ports
            .iter()
            .flat_map(|t| [t.seg_a, t.seg_b])
            .collect();
        p.sort_unstable();
        p.dedup();
        p
    }

    /// The combined admittance matrix over `ports` (as returned by [`Self::ports`]).
    pub fn admittance(&self, ports: &[usize]) -> Vec<Vec<Complex64>> {
        let at = |seg: usize| ports.iter().position(|&p| p == seg).expect("port listed");
        let mut y = vec![vec![ZERO; ports.len()]; ports.len()];
        for t in &self.two_ports {
            let (a, b) = (at(t.seg_a), at(t.seg_b));
            y[a][a] += t.y11;
            y[b][b] += t.y22;
            y[a][b] += t.y12;
            y[b][a] += t.y12;
        }
        y
    }
}

/// Build every `TL` and `NT` in the deck at `freq_hz`.
///
/// A card fnec cannot use is an ERROR, never a skip: skipping it solves a
/// different antenna from the one the deck describes, and reports that answer
/// as this one. The `Ok` side carries informational notes (segment-0 shorthand).
pub fn build_networks(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<(Networks, Vec<String>), String> {
    let mut networks = Networks::default();
    let mut notes = Vec::new();
    for card in &deck.cards {
        match card {
            Card::Tl(tl) => {
                let (tp, n) = crate::tl::tl_two_port(tl, segs, freq_hz)?;
                networks.two_ports.push(tp);
                notes.extend(n);
            }
            Card::Nt(nt) => {
                let (tp, n) = nt_two_port(&nt.raw_fields, segs)?;
                networks.two_ports.push(tp);
                notes.extend(n);
            }
            _ => {}
        }
    }
    notes.sort();
    notes.dedup();
    Ok((networks, notes))
}

fn nt_two_port(f: &[String], segs: &[Segment]) -> Result<(TwoPort, Vec<String>), String> {
    let name = format!("NT {}", f.join(" "));
    if f.len() < 10 {
        return Err(format!(
            "{name}: has {} fields; NT needs 10 (tag1 seg1 tag2 seg2 Y11r Y11i Y12r Y12i \
             Y22r Y22i)",
            f.len()
        ));
    }
    let int = |i: usize| {
        f[i].parse::<f64>()
            .ok()
            .filter(|v| v.fract() == 0.0 && *v >= 0.0)
            .map(|v| v as u32)
            .ok_or_else(|| {
                format!(
                    "{name}: field {} ('{}') is not a segment identifier",
                    i + 1,
                    f[i]
                )
            })
    };
    let float = |i: usize| {
        f[i].parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{name}: field {} ('{}') is not a number", i + 1, f[i]))
    };
    let (tag1, seg1, tag2, seg2) = (int(0)?, int(1)?, int(2)?, int(3)?);
    let mut notes = Vec::new();
    let mut end = |tag: u32, seg: u32| -> Result<usize, String> {
        let (idx, _, note) = find_segment_index(segs, tag, seg)
            .ok_or_else(|| format!("{name}: port ({tag}, {seg}) is not in the geometry"))?;
        notes.extend(note);
        Ok(idx)
    };
    let a = end(tag1, seg1)?;
    let b = end(tag2, seg2)?;
    Ok((
        TwoPort {
            seg_a: a,
            seg_b: b,
            y11: Complex64::new(float(4)?, float(5)?),
            y12: Complex64::new(float(6)?, float(7)?),
            y22: Complex64::new(float(8)?, float(9)?),
        },
        notes,
    ))
}

/// The structure's response combined with its networks.
#[derive(Debug, Clone)]
pub struct NetworkSolution {
    /// Segment currents: the base response plus each undriven port's unit-gap
    /// response, weighted by that port's voltage.
    pub currents: Vec<Complex64>,
    /// `(segment, voltage)` for every UNDRIVEN port, i.e. the weights applied to
    /// the unit-gap responses. A caller superposing anything else that is linear
    /// in the excitation (a right-hand side, homogeneous constants) uses these.
    pub undriven: Vec<(usize, Complex64)>,
    /// `(segment, current)` into the network at every DRIVEN port segment. The
    /// source current there is the segment current plus this.
    pub driven_branch: Vec<(usize, Complex64)>,
}

/// Solve a structure with networks attached, by superposition.
///
/// `base` is the structure's current for the deck's own excitation, `driven` the
/// delta-gap segments with their source voltages, and `unit_gap(q)` the current
/// for a 1 V gap at segment `q` and nothing else, through the SAME solve that
/// produced `base` (same matrix, loads and constraints). Solver-agnostic on
/// purpose: any linear solver can be combined through it.
///
/// For each undriven port `u`, KCL at its gap:
/// `I_u + Σ_j Y[u][j]·V_j = 0`, with `I = base + Σ_q V_q·unit_gap(q)` and `V_j`
/// the source voltage at a driven port. That is a square system in the undriven
/// port voltages.
pub fn solve_with_networks<E>(
    networks: &Networks,
    driven: &[(usize, Complex64)],
    base: &[Complex64],
    mut unit_gap: impl FnMut(usize) -> Result<Vec<Complex64>, E>,
) -> Result<NetworkSolution, NetworkSolveError<E>> {
    let ports = networks.ports();
    let y = networks.admittance(&ports);
    let driven_v = |seg: usize| driven.iter().find(|(s, _)| *s == seg).map(|(_, v)| *v);
    let undriven: Vec<usize> = (0..ports.len())
        .filter(|&i| driven_v(ports[i]).is_none())
        .collect();

    let mut responses = Vec::with_capacity(undriven.len());
    for &i in &undriven {
        responses.push(unit_gap(ports[i]).map_err(NetworkSolveError::Solve)?);
    }

    // Row u: Σ_q (unit_q[u] + Y[u][q]) V_q = −base[u] − Σ_d Y[u][d] V_d.
    let m = undriven.len();
    let mut a = vec![vec![ZERO; m]; m];
    let mut rhs = vec![ZERO; m];
    for (r, &u) in undriven.iter().enumerate() {
        let seg_u = ports[u];
        for (c, &q) in undriven.iter().enumerate() {
            a[r][c] = responses[c][seg_u] + y[u][q];
        }
        rhs[r] = -base[seg_u];
        for (d, &seg_d) in ports.iter().enumerate() {
            if let Some(v) = driven_v(seg_d) {
                rhs[r] -= y[u][d] * v;
            }
        }
    }
    let v_undriven = solve_small(a, rhs).ok_or(NetworkSolveError::Singular)?;

    let mut currents = base.to_vec();
    for (resp, v) in responses.iter().zip(&v_undriven) {
        for (i, r) in currents.iter_mut().zip(resp) {
            *i += v * r;
        }
    }

    // Every port voltage, driven and solved, for the branch currents.
    let v_at = |p: usize| -> Complex64 {
        driven_v(ports[p]).unwrap_or_else(|| {
            let k = undriven.iter().position(|&u| u == p).expect("undriven");
            v_undriven[k]
        })
    };
    let driven_branch = (0..ports.len())
        .filter(|&d| driven_v(ports[d]).is_some())
        .map(|d| {
            let i: Complex64 = (0..ports.len()).map(|j| y[d][j] * v_at(j)).sum();
            (ports[d], i)
        })
        .collect();

    Ok(NetworkSolution {
        currents,
        undriven: undriven
            .iter()
            .zip(&v_undriven)
            .map(|(&u, &v)| (ports[u], v))
            .collect(),
        driven_branch,
    })
}

/// Why [`solve_with_networks`] failed.
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkSolveError<E> {
    /// A unit-gap solve failed.
    Solve(E),
    /// The port-voltage system is singular: the networks short or isolate a port
    /// in a way that leaves its voltage undetermined.
    Singular,
}

/// Gaussian elimination with partial pivoting, for the (tiny) port system.
fn solve_small(mut a: Vec<Vec<Complex64>>, mut b: Vec<Complex64>) -> Option<Vec<Complex64>> {
    let n = b.len();
    let scale = a
        .iter()
        .flatten()
        .map(|z| z.norm())
        .fold(0.0f64, f64::max)
        .max(f64::MIN_POSITIVE);
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].norm().total_cmp(&a[j][col].norm()))?;
        if a[piv][col].norm() <= 1e-13 * scale {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let pivot: Vec<Complex64> = a[col][col..n].to_vec();
        for row in col + 1..n {
            let f = a[row][col] / pivot[0];
            for (x, p) in a[row][col..n].iter_mut().zip(&pivot) {
                *x -= f * p;
            }
            let t = b[col];
            b[row] -= f * t;
        }
    }
    let mut x = vec![ZERO; n];
    for row in (0..n).rev() {
        let s: Complex64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(re: f64, im: f64) -> Complex64 {
        Complex64::new(re, im)
    }

    /// A made-up but consistent "structure": two ports with a known admittance
    /// matrix Ys (unit gap at q gives current Ys[·][q] at the ports), plus a third
    /// segment that is not a port. The network solve must reproduce the circuit
    /// combination Z_in = 1/(Y11 − Y12²/Y22) with Y = Ys + Yn.
    #[test]
    fn a_driven_and_an_undriven_port_combine_as_admittances_in_parallel() {
        let ys = [
            [c(0.004, -0.026), c(0.001, 0.024)],
            [c(0.001, 0.024), c(0.004, -0.026)],
        ];
        let col = |q: usize| vec![ys[0][q], ys[1][q], c(0.5, 0.5)];
        let net = Networks {
            two_ports: vec![TwoPort {
                seg_a: 0,
                seg_b: 1,
                y11: c(0.0, -0.3),
                y12: c(0.0, 0.31),
                y22: c(0.0, -0.3),
            }],
        };
        let sol =
            solve_with_networks::<()>(&net, &[(0, c(1.0, 0.0))], &col(0), |q| Ok(col(q))).unwrap();
        let src = sol.currents[0] + sol.driven_branch[0].1;
        let y = [
            [ys[0][0] + c(0.0, -0.3), ys[0][1] + c(0.0, 0.31)],
            [ys[1][0] + c(0.0, 0.31), ys[1][1] + c(0.0, -0.3)],
        ];
        let want = y[0][0] - y[0][1] * y[1][0] / y[1][1];
        assert!((src - want).norm() < 1e-12, "{src} vs {want}");
        // KCL at the undriven port.
        let (seg, v) = sol.undriven[0];
        assert_eq!(seg, 1);
        let into_network = c(0.0, 0.31) * c(1.0, 0.0) + c(0.0, -0.3) * v;
        assert!((sol.currents[1] + into_network).norm() < 1e-12);
    }

    /// Two networks on the same segment pair add; both ports on one segment is a
    /// one-port of Y11 + Y22 + 2·Y12.
    #[test]
    fn networks_sharing_ports_accumulate() {
        let tp = TwoPort {
            seg_a: 3,
            seg_b: 3,
            y11: c(1.0, 0.0),
            y12: c(0.5, 0.0),
            y22: c(2.0, 0.0),
        };
        let net = Networks {
            two_ports: vec![tp.clone(), tp],
        };
        assert_eq!(net.ports(), vec![3]);
        assert_eq!(net.admittance(&[3]), vec![vec![c(8.0, 0.0)]]);
    }

    #[test]
    fn a_singular_port_system_is_an_error() {
        // No structure coupling and a network that adds nothing: 0·V = −base.
        let net = Networks {
            two_ports: vec![TwoPort {
                seg_a: 1,
                seg_b: 1,
                y11: c(0.0, 0.0),
                y12: c(0.0, 0.0),
                y22: c(0.0, 0.0),
            }],
        };
        let r = solve_with_networks::<()>(&net, &[], &[c(1.0, 0.0), c(0.0, 0.0)], |_| {
            Ok(vec![c(0.0, 0.0), c(0.0, 0.0)])
        });
        assert!(matches!(r, Err(NetworkSolveError::Singular)));
    }
}
