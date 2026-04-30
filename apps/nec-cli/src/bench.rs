use super::solve_session::BenchRecord;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BenchFormat {
    Human,
    Csv,
    Json,
}

pub(super) fn epoch_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn emit_bench_csv_header() {
    eprintln!(
        "bench_csv:timestamp_unix_ms,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,exec,freq_mhz,abs_res,rel_res,diag_spread,sin_rel_res"
    );
}

pub(super) fn emit_bench_record_csv(
    target: &str,
    deck: &str,
    solver: &str,
    run: usize,
    elapsed_ms: u128,
    bench: &BenchRecord,
) {
    eprintln!(
        "bench_csv:{},{},{},{},{},ok,{},{},{},{},{:.6},{:.6e},{:.6e},{:.6e},{:.6e}",
        epoch_millis_now(),
        target,
        deck,
        solver,
        run,
        elapsed_ms,
        bench.mode,
        bench.pulse_rhs,
        bench.exec,
        bench.freq_mhz,
        bench.abs_res,
        bench.rel_res,
        bench.diag_spread,
        bench.sin_rel_res
    );
}

pub(super) fn emit_bench_record_json(
    target: &str,
    deck: &str,
    solver: &str,
    run: usize,
    elapsed_ms: u128,
    bench: &BenchRecord,
) {
    eprintln!(
        "bench_json:{{\"timestamp_unix_ms\":{},\"target\":\"{}\",\"deck\":\"{}\",\"solver\":\"{}\",\"run\":{},\"status\":\"ok\",\"elapsed_ms\":{},\"diag_mode\":\"{}\",\"pulse_rhs\":\"{}\",\"exec\":\"{}\",\"freq_mhz\":{:.6},\"abs_res\":{:.6e},\"rel_res\":{:.6e},\"diag_spread\":{:.6e},\"sin_rel_res\":{:.6e}}}",
        epoch_millis_now(),
        json_escape(target),
        json_escape(deck),
        json_escape(solver),
        run,
        elapsed_ms,
        json_escape(&bench.mode),
        json_escape(&bench.pulse_rhs),
        json_escape(&bench.exec),
        bench.freq_mhz,
        bench.abs_res,
        bench.rel_res,
        bench.diag_spread,
        bench.sin_rel_res
    );
}
