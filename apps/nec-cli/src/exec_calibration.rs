// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! This host's CPU/GPU crossover, measured by `fnec calibrate`.
//!
//! The automatic execution pick used to ship two constants measured on one
//! machine (an RTX 2080 Ti beside a 24-thread CPU) to every user. The crossover is
//! the device's start-up and per-point cost against the CPU's N³ solve, so it
//! belongs to the card, its driver and the CPU beside it — it moved 500 → 600 →
//! 550 in one week on two cards (FND-219). A single host's number is not a
//! property of fnec. So each host measures its own, and a host that has not
//! stays on the CPU: a wrong threshold costs time, never correctness, but an
//! unmeasured one is a guess presented as a measurement.
//!
//! The measurement is of what the decision predicts: whole runs of this binary,
//! `--exec cpu` against `--exec gpu`, on generated λ/2 dipoles — device start-up,
//! adapter enumeration, pipeline compiles and readback included.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// The file's own format.
pub(crate) const SCHEMA: u32 = 1;

/// Bumped by hand when a solver change moves the crossover, so stale calibrations
/// stop applying. Not the fnec version: that would drop every user back to the CPU
/// at each release until they re-ran `fnec calibrate`.
pub(crate) const CALIBRATION_EPOCH: u32 = 1;

/// What a calibration was measured on. A different adapter, driver or CPU moves
/// the crossover, so a calibration applies only where all of these match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HostKey {
    /// The adapter the shared device solves on (not merely the first listed).
    pub(crate) adapter: String,
    pub(crate) backend: String,
    pub(crate) device_type: String,
    pub(crate) driver: String,
    /// The CPU model: the one-point solve is single-threaded, so the same card
    /// beside a different CPU has a different crossover.
    pub(crate) cpu: String,
}

/// One measured size: medians and quartiles of alternating whole-process runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sample {
    /// `"one point"` or `"sweep"`.
    pub(crate) kind: String,
    pub(crate) segments: usize,
    pub(crate) cpu_ms: [f64; 3],
    pub(crate) gpu_ms: [f64; 3],
}

/// A host's calibration, as `fnec calibrate` writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Calibration {
    pub(crate) schema: u32,
    pub(crate) epoch: u32,
    pub(crate) key: HostKey,
    /// The smallest deck the GPU wins on for one frequency point; absent if it won
    /// nowhere up to `measured_up_to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) one_point_min_segs: Option<usize>,
    /// The same for a sweep of one full wave of CPU points (`sweep_threads`) — the
    /// GPU's worst case: more points than threads only adds CPU waves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sweep_min_segs: Option<usize>,
    /// The CPU threads the sweep was measured with; a run with another count is
    /// uncalibrated for sweeps.
    pub(crate) sweep_threads: usize,
    /// The largest size measured. Where the GPU never won, nothing is known beyond.
    pub(crate) measured_up_to: usize,
    #[serde(default)]
    pub(crate) samples: Vec<Sample>,
}

/// Where the calibration lives: `FNEC_EXEC_CALIBRATION`, else
/// `$XDG_CACHE_HOME/fnec/exec-calibration.toml`, else `~/.cache/fnec/…`. A cache:
/// it is derived, and deleting it only means measuring again.
pub(crate) fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FNEC_EXEC_CALIBRATION") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("fnec").join("exec-calibration.toml"))
}

/// The calibration on disk: `Ok(None)` when there is none, `Err` when there is one
/// that cannot be read — said, never silently treated as absent.
pub(crate) fn load() -> Result<Option<Calibration>, String> {
    let Some(p) = path() else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    let cal: Calibration = toml::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    if cal.schema != SCHEMA || cal.epoch != CALIBRATION_EPOCH {
        return Err(format!(
            "{} was written by another fnec (schema {} epoch {}, this one {SCHEMA} / \
             {CALIBRATION_EPOCH}) — run `fnec calibrate` again",
            p.display(),
            cal.schema,
            cal.epoch
        ));
    }
    Ok(Some(cal))
}

/// [`load`], once per process.
pub(crate) fn loaded() -> &'static Result<Option<Calibration>, String> {
    static CAL: std::sync::OnceLock<Result<Option<Calibration>, String>> =
        std::sync::OnceLock::new();
    CAL.get_or_init(load)
}

/// The CPU's model name (`/proc/cpuinfo` on Linux), else the architecture and
/// thread count.
fn cpu_model() -> String {
    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        if let Some(name) = info.lines().find_map(|l| {
            l.strip_prefix("model name")
                .map(|r| r.trim_start_matches([' ', '\t', ':']))
        }) {
            return name.trim().to_string();
        }
    }
    let threads = std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get);
    format!("{} ({threads} threads)", std::env::consts::ARCH)
}

/// This host's key: the adapter the shared device is built on, and the CPU.
/// `None` without a device. Builds the shared device.
pub(crate) fn current_key() -> Option<HostKey> {
    let a = nec_accel::shared_adapter_info()?;
    Some(HostKey {
        adapter: a.name,
        backend: a.backend,
        device_type: a.device_type,
        driver: a.driver,
        cpu: cpu_model(),
    })
}

// ---------------------------------------------------------------------------------
// fnec calibrate
// ---------------------------------------------------------------------------------

const CALIBRATE_USAGE: &str = "Usage: fnec calibrate [--print-key]\n\
    Measures this host's CPU/GPU crossover (whole runs of fnec, --exec cpu against\n\
    --exec gpu, on generated dipoles) and writes it where the automatic --exec pick\n\
    reads it. Takes a minute or two, longer on a slow GPU.";

/// Sizes measured, smallest first. The step is the threshold's error bar; at 100
/// segments near the crossovers measured so far it costs at most ~0.1 s a run.
const GRID: [usize; 12] = [
    100, 200, 300, 400, 500, 600, 700, 800, 1000, 1200, 1500, 2000,
];

/// Alternating runs per size.
const RUNS: usize = 5;

/// Stop once the GPU has won this many sizes in a row.
const WINS_TO_STOP: usize = 3;

/// Stop once a CPU run's median exceeds this.
const CPU_BUDGET_MS: f64 = 4000.0;

pub(crate) fn run(args: &[String]) -> ExitCode {
    let rest: Vec<&str> = args.iter().skip(2).map(String::as_str).collect();
    let print_key = match rest.as_slice() {
        [] => false,
        ["--print-key"] => true,
        _ => {
            eprintln!("{CALIBRATE_USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some(key) = current_key() else {
        eprintln!("fnec calibrate: no GPU device on this host — nothing to calibrate; runs stay on the CPU");
        return if print_key {
            ExitCode::from(3)
        } else {
            ExitCode::SUCCESS
        };
    };
    if print_key {
        match toml::to_string(&key) {
            Ok(t) => {
                print!("{t}");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("fnec calibrate: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(target) = path() else {
        eprintln!("fnec calibrate: no cache directory (set XDG_CACHE_HOME or HOME, or FNEC_EXEC_CALIBRATION)");
        return ExitCode::FAILURE;
    };
    let cal = match measure_and_write(key, &target) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fnec calibrate: {e}");
            return ExitCode::FAILURE;
        }
    };
    for line in summary(&cal) {
        println!("{line}");
    }
    println!("written to {}", target.display());
    ExitCode::SUCCESS
}

/// What a calibration found, one line each.
fn summary(cal: &Calibration) -> [String; 2] {
    let say = |t: Option<usize>| {
        t.map_or(
            format!("never, up to {} segments", cal.measured_up_to),
            |n| format!("from {n} segments"),
        )
    };
    [
        format!(
            "one frequency point: the GPU {}",
            say(cal.one_point_min_segs)
        ),
        format!(
            "a sweep ({} points): the GPU {}",
            cal.sweep_threads,
            say(cal.sweep_min_segs)
        ),
    ]
}

/// Measure this host and write the calibration to `target`, reporting progress on
/// stderr only — the offer runs inside a solve whose stdout may be a deck's output
/// file. Holds `<target>.lock`: two calibrations at once would time each other's
/// CPU load. A successful one removes any decline marker, which could otherwise
/// outlive a calibration deleted later.
pub(crate) fn measure_and_write(key: HostKey, target: &Path) -> Result<Calibration, String> {
    let _lock = CalibrationLock::take(target)?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    let scratch = std::env::temp_dir().join(format!("fnec-calibrate-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let threads = rayon::current_num_threads();
    eprintln!(
        "fnec calibrate: {} ({}, driver {}) beside {} — {threads} threads",
        key.adapter, key.backend, key.driver, key.cpu
    );
    let result = (|| -> Result<Calibration, String> {
        let (one, one_samples, up1) = measure(&exe, &scratch, "one point", 1)?;
        let (sweep, sweep_samples, up2) = measure(&exe, &scratch, "sweep", threads)?;
        let mut samples = one_samples;
        samples.extend(sweep_samples);
        Ok(Calibration {
            schema: SCHEMA,
            epoch: CALIBRATION_EPOCH,
            key,
            one_point_min_segs: one,
            sweep_min_segs: sweep,
            sweep_threads: threads,
            measured_up_to: up1.min(up2),
            samples,
        })
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    let cal = result?;
    write_atomically(target, &cal).map_err(|e| format!("{}: {e}", target.display()))?;
    let _ = std::fs::remove_file(declined_path(target));
    Ok(cal)
}

/// `<target>.lock`, created exclusively; removed on drop. A lock older than
/// [`STALE_LOCK`] is a crashed run's and is taken over. (`File::try_lock` would do
/// this, but is newer than the crate's minimum Rust.)
struct CalibrationLock(PathBuf);

const STALE_LOCK: std::time::Duration = std::time::Duration::from_secs(15 * 60);

impl CalibrationLock {
    fn take(target: &Path) -> Result<Self, String> {
        let p = target.with_extension("lock");
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
            {
                Ok(_) => return Ok(Self(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > STALE_LOCK);
                    if !stale {
                        return Err(format!(
                            "another fnec is calibrating this host ({} exists)",
                            p.display()
                        ));
                    }
                    let _ = std::fs::remove_file(&p);
                }
                Err(e) => return Err(format!("{}: {e}", p.display())),
            }
        }
        Err(format!("cannot take {}", p.display()))
    }
}

impl Drop for CalibrationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// ---------------------------------------------------------------------------------
// The offer: a run with no calibration — or one measured on another GPU, driver or
// CPU — asks, once per host, whether to calibrate now.
// ---------------------------------------------------------------------------------

/// Where a declined offer is remembered: beside the calibration, as the host key it
/// was declined for, so a new GPU, driver or CPU is asked again.
fn declined_path(target: &Path) -> PathBuf {
    target.with_extension("declined")
}

/// The host key an offer was declined for, if any.
pub(crate) fn declined() -> Option<HostKey> {
    let text = std::fs::read_to_string(declined_path(&path()?)).ok()?;
    toml::from_str(&text).ok()
}

/// This host's key without building the device: the adapter the shared device
/// would be built on, its driver, and the CPU. `None` without an adapter.
fn current_key_cheap() -> Option<HostKey> {
    let a = nec_accel::preferred_adapter_info()?;
    Some(HostKey {
        adapter: a.name,
        backend: a.backend,
        device_type: a.device_type,
        driver: a.driver,
        cpu: cpu_model(),
    })
}

/// What to ask, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Offer {
    /// No calibration on this host.
    Calibrate,
    /// A calibration measured on another GPU, driver or CPU.
    Recalibrate(HostKey),
}

/// Whether to offer, from the calibration on disk, this host's key and a decline.
/// Not for an unreadable calibration (its reason already names the file), not where
/// the calibration is this host's, and not where this host's key was declined.
pub(crate) fn offer(
    current: &Result<Option<Calibration>, String>,
    key: &HostKey,
    declined: Option<&HostKey>,
) -> Option<Offer> {
    if declined == Some(key) {
        return None;
    }
    match current {
        Err(_) => None,
        Ok(None) => Some(Offer::Calibrate),
        Ok(Some(c)) if c.key == *key => None,
        Ok(Some(c)) => Some(Offer::Recalibrate(c.key.clone())),
    }
}

/// Offer to calibrate, on an interactive run of a deck the calibration could
/// matter for. Returns the new calibration if the user said yes and it was
/// measured; otherwise `None`, and the run goes on with what it had. A "no" is
/// remembered for this host's key; a calibration that could not run (another one
/// holds the lock) is not a "no" and records nothing.
pub(crate) fn maybe_offer(
    segments: usize,
    current: &Result<Option<Calibration>, String>,
) -> Option<Calibration> {
    use std::io::{BufRead, IsTerminal, Write};
    // Below the smallest size calibrated no host measured so far picks the GPU, so
    // the answer could not change this run — and a "no" is final for the host.
    if segments < GRID[0] || !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return None;
    }
    // Nowhere to remember a "no" would mean asking on every run.
    let target = path()?;
    let key = current_key_cheap()?;
    let ask = offer(current, &key, declined().as_ref())?;
    let question = match &ask {
        Offer::Calibrate => format!(
            "fnec: no CPU/GPU calibration for this host ({}, driver {}, {}).",
            key.adapter, key.driver, key.cpu
        ),
        Offer::Recalibrate(old) => format!(
            "fnec: the GPU, driver or CPU changed since this host was calibrated ({}, driver {}, {} → {}, driver {}, {}).",
            old.adapter, old.driver, old.cpu, key.adapter, key.driver, key.cpu
        ),
    };
    eprint!(
        "{question}
      Calibrate now? It takes a minute or two, longer on a slow GPU, and lets runs without --exec use the GPU where it is faster. [y/N] "
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    let yes = std::io::stdin().lock().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !yes {
        match toml::to_string(&key)
            .map_err(|e| e.to_string())
            .and_then(|t| std::fs::write(declined_path(&target), t).map_err(|e| e.to_string()))
        {
            Ok(()) => eprintln!(
                "fnec: not asking again on this host; `fnec calibrate` measures it any time"
            ),
            Err(e) => eprintln!("fnec: could not remember the answer: {e}"),
        }
        return None;
    }
    match measure_and_write(key, &target) {
        Ok(cal) => {
            for line in summary(&cal) {
                eprintln!("fnec calibrate: {line}");
            }
            eprintln!("fnec calibrate: written to {}", target.display());
            Some(cal)
        }
        Err(e) => {
            eprintln!("fnec calibrate: {e}; this run stays on the CPU");
            None
        }
    }
}

/// Measure one kind over [`GRID`]: the threshold (the first size from which the
/// GPU's slowest quartile beats the CPU's fastest at every size measured after it),
/// the samples, and the largest size measured.
fn measure(
    exe: &Path,
    scratch: &Path,
    kind: &str,
    points: usize,
) -> Result<(Option<usize>, Vec<Sample>, usize), String> {
    let mut samples = Vec::new();
    let mut wins_in_row = 0;
    for &n in &GRID {
        let deck = scratch.join(format!("dipole-{n}-{points}.nec"));
        std::fs::write(&deck, dipole(n, points)).map_err(|e| format!("{}: {e}", deck.display()))?;
        let (mut cpu, mut gpu) = (Vec::new(), Vec::new());
        let mut declined = 0;
        for _ in 0..RUNS {
            cpu.push(time_run(exe, &deck, "cpu")?.0);
            let (ms, on_device) = time_run(exe, &deck, "gpu")?;
            if on_device {
                gpu.push(ms);
            } else {
                declined += 1;
            }
        }
        if gpu.is_empty() {
            return Err(format!(
                "the device declined every {kind} run at {n} segments — not a host to calibrate"
            ));
        }
        let (c, g) = (quartiles(&mut cpu), quartiles(&mut gpu));
        eprintln!(
            "  {kind:9} {n:5} segs: CPU {:7.0} ms   GPU {:7.0} ms{}",
            c[1],
            g[1],
            if declined > 0 {
                format!("   ({declined} GPU runs fell back, dropped)")
            } else {
                String::new()
            }
        );
        samples.push(Sample {
            kind: kind.to_string(),
            segments: n,
            cpu_ms: c,
            gpu_ms: g,
        });
        wins_in_row = if gpu_wins(samples.last().expect("just pushed")) {
            wins_in_row + 1
        } else {
            0
        };
        if wins_in_row >= WINS_TO_STOP || c[1] > CPU_BUDGET_MS {
            break;
        }
    }
    let up_to = samples.last().map_or(0, |s| s.segments);
    Ok((threshold(&samples), samples, up_to))
}

/// The first size from which the GPU wins at every size measured after it.
pub(crate) fn threshold(samples: &[Sample]) -> Option<usize> {
    let wins: Vec<bool> = samples.iter().map(gpu_wins).collect();
    (0..samples.len())
        .find(|&i| wins[i..].iter().all(|&w| w))
        .map(|i| samples[i].segments)
}

/// The GPU wins a size when its slowest quartile beats the CPU's fastest — one
/// rule for the threshold and the early stop.
fn gpu_wins(s: &Sample) -> bool {
    s.gpu_ms[2] < s.cpu_ms[0]
}

/// Lower quartile, median, upper quartile.
fn quartiles(v: &mut [f64]) -> [f64; 3] {
    v.sort_by(f64::total_cmp);
    let q = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
    [q(0.25), q(0.5), q(0.75)]
}

/// One whole run of this binary: its wall time, and whether the device solved it
/// (the run's own decision record, not the flag it was given).
fn time_run(exe: &Path, deck: &Path, exec: &str) -> Result<(f64, bool), String> {
    let start = Instant::now();
    let out = Command::new(exe)
        .args(["--exec", exec])
        .arg(deck)
        .output()
        .map_err(|e| format!("running {}: {e}", exe.display()))?;
    let ms = start.elapsed().as_secs_f64() * 1e3;
    if !out.status.success() {
        return Err(format!(
            "--exec {exec} on {} failed:\n{}",
            deck.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let on_device = String::from_utf8_lossy(&out.stderr).contains("solve_exec=gpu");
    Ok((ms, on_device))
}

/// A λ/2 dipole at 14.2 MHz with `n` segments, fed at the centre, swept over
/// `points` frequencies.
fn dipole(n: usize, points: usize) -> String {
    format!(
        "CE\nGW 1 {n} 0 0 -5.28 0 0 5.28 .0005\nGE 0\nEX 0 1 {} 0 1 0\nFR 0 {points} 0 0 14.2 0.01\nEN\n",
        n / 2 + 1
    )
}

/// Write `cal` to `target` through a temporary file and a rename, so a reader never
/// sees half a calibration.
fn write_atomically(target: &Path, cal: &Calibration) -> Result<(), String> {
    let dir = target.parent().ok_or("no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = toml::to_string(cal).map_err(|e| e.to_string())?;
    let tmp = target.with_extension(format!("toml.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: usize, cpu: f64, gpu: f64) -> Sample {
        Sample {
            kind: "one point".into(),
            segments: n,
            cpu_ms: [cpu - 5.0, cpu, cpu + 5.0],
            gpu_ms: [gpu - 5.0, gpu, gpu + 5.0],
        }
    }

    /// The threshold is the first size from which the GPU wins at every later
    /// size, with the quartiles clear — a noisy win below it does not count.
    #[test]
    fn the_threshold_is_where_the_gpu_wins_from_then_on() {
        let t = |v: &[Sample]| threshold(v);
        assert_eq!(
            t(&[
                s(100, 50.0, 400.0),
                s(500, 330.0, 400.0),
                s(600, 570.0, 415.0),
                s(700, 800.0, 420.0)
            ]),
            Some(600)
        );
        // A win at 300 that does not hold at 400 is noise.
        assert_eq!(
            t(&[
                s(300, 420.0, 400.0),
                s(400, 410.0, 405.0),
                s(500, 600.0, 410.0)
            ]),
            Some(500)
        );
        // Quartiles that overlap are not a win.
        assert_eq!(t(&[s(500, 410.0, 404.0), s(600, 600.0, 410.0)]), Some(600));
        // Never: an integrated GPU.
        assert_eq!(t(&[s(500, 330.0, 900.0), s(2000, 4100.0, 5000.0)]), None);
    }

    fn key(gpu: &str, driver: &str) -> HostKey {
        HostKey {
            adapter: gpu.into(),
            backend: "Vulkan".into(),
            device_type: "DiscreteGpu".into(),
            driver: driver.into(),
            cpu: "Some CPU".into(),
        }
    }

    fn calibrated(k: HostKey) -> Calibration {
        Calibration {
            schema: SCHEMA,
            epoch: CALIBRATION_EPOCH,
            key: k,
            one_point_min_segs: Some(600),
            sweep_min_segs: Some(600),
            sweep_threads: 8,
            measured_up_to: 800,
            samples: vec![],
        }
    }

    /// The offer: on a host with no calibration, or one measured on another GPU,
    /// driver or CPU — unless this host's key was declined. A change of GPU, driver
    /// or CPU after a decline is asked again.
    #[test]
    fn the_offer_follows_the_host_key() {
        let here = key("RTX", "595");
        assert_eq!(offer(&Ok(None), &here, None), Some(Offer::Calibrate));
        assert_eq!(
            offer(&Ok(Some(calibrated(here.clone()))), &here, None),
            None
        );
        let old = key("RTX", "580");
        assert_eq!(
            offer(&Ok(Some(calibrated(old.clone()))), &here, None),
            Some(Offer::Recalibrate(old.clone()))
        );
        // Declined for this key: not asked again.
        assert_eq!(offer(&Ok(None), &here, Some(&here)), None);
        assert_eq!(
            offer(&Ok(Some(calibrated(old.clone()))), &here, Some(&here)),
            None
        );
        // Declined for the old driver: the new one is asked.
        assert_eq!(offer(&Ok(None), &here, Some(&old)), Some(Offer::Calibrate));
        // An unreadable calibration is named by the pick, not offered over.
        assert_eq!(offer(&Err("bad".into()), &here, None), None);
    }

    /// The file round-trips, and an absent threshold stays absent (it means
    /// "never", not zero).
    #[test]
    fn a_calibration_round_trips_through_its_file() {
        let cal = Calibration {
            schema: SCHEMA,
            epoch: CALIBRATION_EPOCH,
            key: HostKey {
                adapter: "Some GPU".into(),
                backend: "Vulkan".into(),
                device_type: "DiscreteGpu".into(),
                driver: "1.2.3".into(),
                cpu: "Some CPU".into(),
            },
            one_point_min_segs: Some(550),
            sweep_min_segs: None,
            sweep_threads: 8,
            measured_up_to: 2000,
            samples: vec![s(550, 445.0, 408.0)],
        };
        let text = toml::to_string(&cal).unwrap();
        assert!(!text.contains("sweep_min_segs"), "{text}");
        let back: Calibration = toml::from_str(&text).unwrap();
        assert_eq!(back, cal);
    }
}
