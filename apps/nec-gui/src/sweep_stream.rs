// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The streaming sweep task, extracted so its messages can be tested.
//!
//! This body lived inline in `FnecGui::update`, where nothing could reach it:
//! deleting any of its `send` calls left the whole suite green, so the caveats
//! #398 and #400 added were carried by review alone (FND-034). It captures no
//! `self` — only the deck text, the sweep bounds and the output sink — so lifting
//! it out costs nothing and makes every message it emits assertable.

use iced::futures::{Sink, SinkExt};

use crate::app_state::Message;
use crate::solve::SweepJob;

/// Run a sweep, emitting messages as it goes.
///
/// The order is the contract, and each part of it was a bug once:
///
/// 1. `SweepCaveats` with the deck's geometry caveats, **before** the first point,
///    so a long sweep does not withhold "the antenna is too low" until it ends
///    (FND-042).
/// 2. `SweepPointComputed` per frequency, so the chart fills in live.
/// 3. `SweepCaveats` with the negative-resistance aggregate — on **both** the
///    success and the mid-sweep-failure path. The failure path used to return
///    without it, so points already on screen were never qualified (FND-014).
/// 4. `SweepStreamDone`, or `SweepComplete(Err)` if the sweep failed.
pub async fn run_sweep_stream(
    deck_text: String,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
    output: &mut (impl Sink<Message, Error = impl std::fmt::Debug> + Unpin),
) {
    let job = match SweepJob::prepare(&deck_text, start_mhz, end_mhz, step_mhz) {
        Ok(job) => job,
        Err(e) => {
            let _ = output.send(Message::SweepComplete(Err(e))).await;
            return;
        }
    };

    let _ = output
        .send(Message::SweepCaveats(job.geometry_caveats()))
        .await;

    // Accumulated here because the job holds the geometry and the UI thread does
    // not, so the aggregate has to be computed on this side.
    let mut seen = Vec::new();
    for &f in job.freqs_mhz() {
        match job.solve_at(f) {
            Ok(pt) => {
                seen.push(pt.clone());
                let _ = output.send(Message::SweepPointComputed(pt)).await;
            }
            Err(e) => {
                let _ = output
                    .send(Message::SweepCaveats(
                        job.negative_resistance_caveat(&seen).into_iter().collect(),
                    ))
                    .await;
                let _ = output.send(Message::SweepComplete(Err(e))).await;
                return;
            }
        }
    }

    let _ = output
        .send(Message::SweepCaveats(
            job.negative_resistance_caveat(&seen).into_iter().collect(),
        ))
        .await;
    let _ = output.send(Message::SweepStreamDone).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::channel::mpsc;
    use iced::futures::StreamExt;

    /// Drive the stream to completion and collect everything it sent.
    fn run(deck: &str, start: f64, end: f64, step: f64) -> Vec<Message> {
        let (mut tx, rx) = mpsc::channel::<Message>(256);
        let deck = deck.to_string();
        iced::futures::executor::block_on(async move {
            run_sweep_stream(deck, start, end, step, &mut tx).await;
            drop(tx);
            rx.collect::<Vec<_>>().await
        })
    }

    fn caveats(msgs: &[Message]) -> Vec<&Vec<String>> {
        msgs.iter()
            .filter_map(|m| match m {
                Message::SweepCaveats(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    // 0.634 m over GN 2: 0.030 lambda at 14.2 MHz, well below the 0.1 lambda
    // threshold, so every point of this sweep earns the low-ground caveat.
    const LOW_DIPOLE: &str = "CM low over ground\nCE\nGW 1 21 -5.282 0 0.634 5.282 0 0.634 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    const CLEAN: &str = "CM plain dipole\nCE\nGW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    #[test]
    fn the_geometry_caveats_arrive_before_the_first_point() {
        // The reason this send exists: a user watching a long sweep should not
        // learn only at the end that the antenna was too low the whole way
        // (FND-042). Ordering is the assertion — a caveat sent at the end would
        // still be "present" but would not do its job.
        let msgs = run(LOW_DIPOLE, 14.0, 14.4, 0.1);
        let first_caveat = msgs
            .iter()
            .position(|m| matches!(m, Message::SweepCaveats(c) if !c.is_empty()))
            .expect("a caveat");
        let first_point = msgs
            .iter()
            .position(|m| matches!(m, Message::SweepPointComputed(_)))
            .expect("a point");
        assert!(
            first_caveat < first_point,
            "caveats must precede the first point: {msgs:?}"
        );
        assert!(
            caveats(&msgs)[0]
                .iter()
                .any(|w| w.contains("above finite ground")),
            "{:?}",
            caveats(&msgs)[0]
        );
    }

    #[test]
    fn a_completed_sweep_ends_with_stream_done() {
        let msgs = run(CLEAN, 14.0, 14.4, 0.1);
        assert!(
            matches!(msgs.last(), Some(Message::SweepStreamDone)),
            "{msgs:?}"
        );
        assert_eq!(
            msgs.iter()
                .filter(|m| matches!(m, Message::SweepPointComputed(_)))
                .count(),
            5
        );
        // A clean deck still sends the caveat messages, both empty — the receiver
        // appends, so an empty send is the honest "nothing to add".
        assert_eq!(caveats(&msgs).len(), 2);
        assert!(caveats(&msgs).iter().all(|c| c.is_empty()));
    }

    #[test]
    fn a_deck_that_cannot_be_prepared_reports_the_failure_and_nothing_else() {
        // `prepare` rejects the geometry the CLI rejects, so this is the path a
        // crossing-wires deck takes.
        let msgs = run(CLEAN, 14.4, 14.0, 0.1); // start >= end
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert!(matches!(msgs[0], Message::SweepComplete(Err(_))));
    }

    #[test]
    fn a_sweep_with_no_solvable_points_still_reports_its_caveats() {
        // The mid-sweep failure path used to `return` without sending the
        // aggregate, so points already on screen were never qualified (FND-014).
        // A deck whose geometry prepares but whose solve fails exercises it.
        const NO_EX: &str = "CM no excitation\nCE\nGW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nFR 0 1 0 0 14.2 0\nEN\n";
        let msgs = run(NO_EX, 14.0, 14.2, 0.1);
        assert!(
            matches!(msgs.last(), Some(Message::SweepComplete(Err(_)))),
            "{msgs:?}"
        );
        // Two `SweepCaveats`, not merely "at least one": the geometry send fires
        // before the loop regardless, so `any()` is satisfied even with the
        // failure-path aggregate deleted — which is exactly the send this test
        // exists to pin. Counting is what distinguishes them.
        assert_eq!(
            caveats(&msgs).len(),
            2,
            "the failure path must send its aggregate as well as the geometry \
             caveats: {msgs:?}"
        );
        assert!(
            !msgs.iter().any(|m| matches!(m, Message::SweepStreamDone)),
            "a failed sweep must not report completion: {msgs:?}"
        );
    }
}
