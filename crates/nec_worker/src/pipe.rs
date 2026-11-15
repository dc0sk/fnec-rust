// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The child-process pipe a worker handle talks over, with deadlines.
//!
//! Both `LocalWorkerHandle` and `SshWorkerHandle` carried the same three fields
//! — a `Child`, its `ChildStdin`, a `BufReader<ChildStdout>` — and their own
//! copy of write-then-`read_line`. That is the repo's dominant defect class in
//! progress, and it had already diverged: a task that failed to serialise was a
//! `Task` fault on the local path and a `Worker` fault on the SSH one, so an
//! unserialisable task evicted a healthy remote worker (FND-136). One core, so
//! they cannot differ again.
//!
//! **Why a reader thread.** `read_line` on a child pipe has no timeout in std,
//! and a worker that accepts a task and never answers wedged it forever
//! (FND-101) — with `dispatch_batch`'s `thread::scope` then withholding the
//! whole batch, including other workers' finished results. The fix is one
//! long-lived thread per worker, spawned once, pumping lines into a channel;
//! `dispatch` becomes a write plus `recv_timeout`.
//!
//! One thread per WORKER, not per dispatch. A thread per dispatch would leak one
//! for every wedged worker; this one exits on its own, because a timeout evicts
//! the handle, `Drop` kills the child, and killing the child closes the pipe,
//! which ends the thread's read at EOF.
//!
//! **The read is bounded (FND-331).** The reader thread used `lines()`, which
//! grows a `String` until the line ends: a worker writing an endless line
//! aborted the controller on out-of-memory, the deadline bounding the wait but
//! not the allocation. It now calls the protocol's one bounded read,
//! [`read_bounded_line`] — the worker's own side's read since FND-096 — and
//! pushes a [`Line`]: the text, or `TooLong` for a line past
//! [`MAX_LINE_BYTES`], whose tail has been drained so the stream is back in
//! sync. A `TooLong` names no task, so it is a stray line (below): skipped, and
//! counted toward [`MAX_STRAY_LINES`].
//!
//! **An answer is paired with the task it names (FND-326).** Both handles took
//! the next line as the answer to the task just written. A worker's stdout is
//! shared with whatever the remote login shell prints — one `.bashrc` `echo`
//! was read as task 0's answer, task 0's answer as task 1's, and every later
//! impedance of the sweep printed against the next frequency, with no word.
//! [`WorkerPipe::answer_to`] reads until a line names the waiting task: a line
//! that is not a result, a result naming another task and an over-long line are
//! strays — skipped, warned about once per connection, and capped. A worker's
//! `"unknown"` ([`UNKNOWN_TASK_ID`]) answers the waiting task, because one task
//! is in flight per worker and that is the protocol's word for "I could not read
//! it". A solved result must also carry the task's `frequency_hz`: the id names
//! the task, the frequency is the number the user reads beside it.
//!
//! **And a writer thread (FND-244).** A child that stops draining stdin blocked
//! the task write forever once the line passed the pipe buffer (64 KiB here; a
//! many-wire deck's base64 task line can). The write now runs on a second
//! long-lived thread that owns stdin, and `send` waits for its acknowledgement
//! up to a deadline — the same shape as the read. The writer is detached and is
//! never joined by `kill`, `shutdown` or any `Drop`: a write blocked on a pipe
//! whose read end a GRANDCHILD also holds stays blocked after the child is
//! killed (measured: it returned only when the grandchild exited), so a join
//! there would turn a bounded eviction into a hang. It ends when the pipe's read
//! end closes (the write fails) or its job channel closes (the handle is gone).
//!
//! A task line whose write did not complete was not delivered: the protocol is
//! line-framed and the worker acts on a newline, so whatever stopped the reading
//! did not start from this task. Every send failure is therefore
//! [`DispatchError::Unreachable`], which evicts the worker without spending the
//! task's retry budget (FND-102); only a broken pipe is a connection a reconnect
//! can mend ([`SendError::reconnects`]). After a timed-out write the pipe is
//! poisoned — the stuck job still heads the writer's queue, and a later line
//! would wait behind it.

use std::io::{BufReader, Write};
use std::process::Child;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{
    read_bounded_line, BoundedRead, TaskMessage, TaskResult, MAX_LINE_BYTES, UNKNOWN_TASK_ID,
};
use crate::DispatchError;

/// How long to wait for a worker's answer to one solve task.
///
/// Matched to the kernel's TCP retransmission bound, which is what already
/// limited the *dead-socket* case: the guarantee this makes is "never worse than
/// a dead socket, and bounded where it was infinite". It is also more than five
/// times the slowest legitimate point measured on this hardware — an MPIE sweep
/// point runs in minutes — because a deadline that fires on a slow but healthy
/// solve would be a worse defect than the hang it replaces. A false fire costs
/// one eviction and one retried task: duplicated work, never a wrong number.
pub const DEFAULT_SOLVE_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// How long to wait for a capability probe, which solves a trivial deck.
///
/// Short on purpose and separate from the solve deadline: `connect_all` probes
/// hosts SERIALLY at startup, before any pool exists, so one wedged host used to
/// block the run before it began.
pub const PROBE_DEADLINE: Duration = Duration::from_secs(30);

/// How long a worker gets to exit after being told to, before it is killed.
///
/// The design doc has promised "up to 2 seconds for graceful exit" in prose
/// while `shutdown` called an unbounded `wait()`. Worse, the distributed sweep
/// shuts the pool down BEFORE printing, so a worker wedged here hid a sweep that
/// had already succeeded (FND-137).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// How many stray lines one task may read past before the worker is judged not
/// to speak the protocol: a `Worker` fault, so it is evicted and the task takes
/// a strike, as for a worker that took the task and never answered (FND-326).
///
/// 64: a dotfile banner is one to ten lines, and a worker that writes 64 lines
/// in a row that answer nothing is not a worker. The deadline bounds the skip
/// loop in time; this bounds it in lines — and with the line bound (FND-331) in
/// memory — so a stream of junk is evicted at once instead of being read for the
/// whole solve deadline.
pub const MAX_STRAY_LINES: usize = 64;

/// How many lines the reader thread may hold that no task has read yet.
///
/// The stray cap bounds what one task READS; it did not bound what the reader
/// thread QUEUED, in an unbounded channel, while the task was reading it or no
/// task was waiting at all. A worker writing `yes` filled 12 GB in under a
/// minute with the cap removed (FND-326's sabotage, stopped by `ulimit -v`), and
/// with the cap in place still queued as fast as it wrote until the eviction. A
/// bounded channel makes the reader wait, and the child with it: a healthy
/// worker writes one line per task, so it never comes near this (FND-356).
///
/// Why 65: a task the cap tolerates reads at most `MAX_STRAY_LINES - 1` strays
/// and then its answer, so a queue of `MAX_STRAY_LINES + 1` — the +1 is the
/// answer — never makes a worker the cap would keep wait on the controller.
///
/// Worst case, each line at the protocol maximum: 65 × [`MAX_LINE_BYTES`] ≈
/// 260 MiB per worker, bounded, where the unbounded channel had no bound.
const QUEUED_LINES: usize = MAX_STRAY_LINES + 1;

/// How many stray lines the warning quotes.
const QUOTED_STRAYS: usize = 3;

/// How much of one stray line the warning quotes.
const QUOTE_BYTES: usize = 120;

/// Why a line could not be handed to the worker. Every case means the line was
/// not delivered, so every case is [`DispatchError::Unreachable`].
#[derive(Debug)]
pub enum SendError {
    /// The write failed: the pipe is broken (the child or the connection is gone).
    Broken(String),
    /// The child did not take the line within the deadline: it stopped reading.
    Timeout(Duration),
    /// An earlier write timed out and still heads the writer's queue.
    Poisoned,
    /// The writer thread is gone.
    WriterGone,
}

impl SendError {
    /// Whether a reconnect can mend this: a broken connection, yes; a host that
    /// stopped reading has already spent a deadline, and a resend would spend a
    /// second one on it.
    pub fn reconnects(&self) -> bool {
        matches!(self, SendError::Broken(_))
    }
}

impl From<SendError> for DispatchError {
    fn from(e: SendError) -> Self {
        DispatchError::Unreachable(match e {
            SendError::Broken(m) => m,
            SendError::Timeout(d) => {
                format!(
                    "worker did not take its task within {:.1}s",
                    d.as_secs_f64()
                )
            }
            SendError::Poisoned => "an earlier task line is still unwritten".to_string(),
            SendError::WriterGone => "the worker's writer thread is gone".to_string(),
        })
    }
}

/// One line from the worker, as the reader thread hands it over.
#[derive(Debug)]
enum Line {
    /// A complete line, without its newline.
    Text(String),
    /// A line past [`MAX_LINE_BYTES`]: not kept, its tail drained.
    TooLong,
}

/// What one line from the worker is, to the task waiting on it.
#[derive(Debug)]
enum Reading {
    /// The task's answer.
    Answer(TaskResult),
    /// The task's answer, and unusable: a `Task` fault, the worker stays.
    Unusable(String),
    /// Not the task's answer, described for the warning.
    Stray(String),
}

/// A stray line as the warning quotes it: cut to [`QUOTE_BYTES`] on a character
/// boundary, quoted, every non-printing character escaped.
fn quote(text: &str) -> String {
    if text.len() <= QUOTE_BYTES {
        return format!("{text:?}");
    }
    let mut end = QUOTE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}…", &text[..end])
}

/// Pair one line with `task`, or say why it is not its answer.
///
/// Who a line answers is read before what it says: a line naming this task that
/// is not a readable result is this task's UNUSABLE answer (FND-117) — skipping
/// it would wait out the whole deadline and then evict a healthy worker over one
/// field an older or newer build spells differently.
fn classify(line: Line, task: &TaskMessage) -> Reading {
    let text = match line {
        Line::TooLong => {
            return Reading::Stray(format!(
                "a line past the {MAX_LINE_BYTES}-byte protocol maximum"
            ))
        }
        Line::Text(t) => t,
    };
    let text = text.trim();
    let named = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("task_id")?.as_str().map(str::to_owned));
    let Some(id) = named else {
        return Reading::Stray(quote(text));
    };
    if id != task.task_id && id != UNKNOWN_TASK_ID {
        return Reading::Stray(format!("a result for task {}", quote(&id)));
    }
    let result: TaskResult = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => return Reading::Unusable(format!("unreadable result from worker: {e}")),
    };
    if let TaskResult::Ok {
        task_id,
        frequency_hz,
        ..
    } = &result
    {
        if task_id == UNKNOWN_TASK_ID {
            return Reading::Unusable(format!(
                "it sent a solved result for task '{UNKNOWN_TASK_ID}', which names no \
                 task; the number is refused"
            ));
        }
        // Exact: the worker copies the f64 it was sent, and serde_json writes a
        // finite f64 so that it reads back to the same value. An `Error` is not
        // checked — it legitimately carries 0.0 when the task was unreadable.
        if *frequency_hz != task.frequency_hz {
            return Reading::Unusable(format!(
                "it answered task '{}' at {frequency_hz} Hz for a task at {} Hz",
                task.task_id, task.frequency_hz
            ));
        }
    }
    Reading::Answer(result)
}

/// One line for the writer thread, and where to say how the write went.
struct Job {
    line: String,
    ack: Sender<std::io::Result<()>>,
}

/// A worker subprocess, a thread writing its stdin and a thread pumping its
/// stdout into a channel.
#[derive(Debug)]
pub struct WorkerPipe {
    child: Child,
    jobs: Sender<Job>,
    lines: Receiver<Line>,
    poisoned: bool,
    /// Every stray line this pipe has skipped — the tripwire a test reads.
    strays: usize,
    /// Whether this worker's strays have been warned about yet: once per
    /// connection, since a reconnect builds a fresh pipe.
    warned: bool,
    /// Every line the reader thread has handed over — the tripwire for the
    /// queue's bound.
    queued: Arc<AtomicUsize>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Job({} bytes)", self.line.len())
    }
}

impl WorkerPipe {
    /// Take ownership of a spawned child and start pumping its stdout.
    pub fn new(mut child: Child) -> Self {
        let mut stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = child.stdout.take().expect("stdout must be piped");
        let (jobs, queue) = mpsc::channel::<Job>();
        // Detached, never joined (see the module doc).
        std::thread::spawn(move || {
            for job in queue {
                // `writeln!` is `write_all`: a write cut short when the reader
                // went away returns the partial count first, and the loop's next
                // write surfaces the broken pipe.
                let r = writeln!(stdin, "{}", job.line).and_then(|()| stdin.flush());
                // Nobody waiting any more (the send timed out): drop the answer.
                let _ = job.ack.send(r);
            }
        });
        // Bounded (see QUEUED_LINES).
        let (tx, lines) = mpsc::sync_channel(QUEUED_LINES);
        let queued = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&queued);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();
            loop {
                // Bounded, not `lines()` (FND-331, see the module doc).
                let item = match read_bounded_line(&mut reader, &mut buf) {
                    Ok(BoundedRead::Eof) | Err(_) => return,
                    Ok(BoundedRead::TooLong) => Line::TooLong,
                    Ok(BoundedRead::Line) => {
                        // What `lines()` delivered: the line without "\n" or
                        // "\r\n", and a line that is not UTF-8 ends the read.
                        if buf.last() == Some(&b'\n') {
                            buf.pop();
                            if buf.last() == Some(&b'\r') {
                                buf.pop();
                            }
                        }
                        match String::from_utf8(std::mem::take(&mut buf)) {
                            Ok(l) => Line::Text(l),
                            Err(_) => return,
                        }
                    }
                };
                // A closed receiver means the handle is gone; stop reading.
                if tx.send(item).is_err() {
                    return;
                }
                counted.fetch_add(1, Ordering::Relaxed);
            }
        });
        Self {
            child,
            jobs,
            lines,
            poisoned: false,
            strays: 0,
            warned: false,
            queued,
        }
    }

    /// Hand one line to the worker, waiting up to `deadline` for it to be written.
    ///
    /// Every failure is a line not delivered — `Unreachable`, not `Worker`, so the
    /// task is not blamed for a worker that never received it (FND-102, FND-244).
    pub fn send(&mut self, line: &str, deadline: Duration) -> Result<(), SendError> {
        if self.poisoned {
            return Err(SendError::Poisoned);
        }
        let (ack, answer) = mpsc::channel();
        self.jobs
            .send(Job {
                line: line.to_string(),
                ack,
            })
            .map_err(|_| SendError::WriterGone)?;
        match answer.recv_timeout(deadline) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(SendError::Broken(e.to_string())),
            Err(RecvTimeoutError::Timeout) => {
                self.poisoned = true;
                Err(SendError::Timeout(deadline))
            }
            Err(RecvTimeoutError::Disconnected) => Err(SendError::WriterGone),
        }
    }

    /// The answer to `task` — the line that names it — within `deadline` after
    /// `start`, the task's one deadline across its write and this read
    /// (FND-101). `worker` labels the warning.
    ///
    /// Stray lines are skipped (see the module doc): the first time a worker
    /// writes any, one warning quotes them. A worker that writes
    /// [`MAX_STRAY_LINES`] of them for one task is a `Worker` fault. A timeout or
    /// a closed stdout while skipping is the `Worker` fault it always was.
    pub fn answer_to(
        &mut self,
        task: &TaskMessage,
        worker: &str,
        start: Instant,
        deadline: Duration,
    ) -> Result<TaskResult, DispatchError> {
        let mut skipped = 0usize;
        let mut quoted: Vec<String> = Vec::new();
        loop {
            let line = match self.recv_line(start, deadline) {
                Ok(line) => line,
                Err(DispatchError::Worker(m)) if skipped > 0 => {
                    return Err(DispatchError::Worker(format!(
                        "{m}, after {skipped} line{} that answered nothing",
                        if skipped == 1 { "" } else { "s" }
                    )))
                }
                Err(e) => return Err(e),
            };
            let answer = match classify(line, task) {
                Reading::Answer(result) => Ok(result),
                Reading::Unusable(m) => Err(DispatchError::Task(m)),
                Reading::Stray(what) => {
                    skipped += 1;
                    self.strays += 1;
                    if quoted.len() < QUOTED_STRAYS {
                        quoted.push(what);
                    }
                    if skipped >= MAX_STRAY_LINES {
                        return Err(DispatchError::Worker(format!(
                            "it wrote {skipped} lines, none the answer to task '{}' — it is \
                             not speaking the protocol; the first: {}",
                            task.task_id,
                            quoted.join(", ")
                        )));
                    }
                    // A stray does not extend the deadline. `recv_timeout` of
                    // what is left returns a QUEUED line even when nothing is
                    // left, so without this a worker that keeps lines coming
                    // is never timed out.
                    if start.elapsed() >= deadline {
                        return Err(DispatchError::Worker(format!(
                            "worker did not answer within {:.1}s, after {skipped} line{} that \
                             answered nothing",
                            deadline.as_secs_f64(),
                            if skipped == 1 { "" } else { "s" }
                        )));
                    }
                    continue;
                }
            };
            if skipped > 0 && !self.warned {
                self.warned = true;
                eprintln!(
                    "warning: worker '{worker}' wrote {skipped} line{} that {} not the answer \
                     to task '{}' before answering it; ignored: {}{}. Keep the worker \
                     host's non-interactive shell startup silent on stdout \
                     (docs/worker-deployment.md)",
                    if skipped == 1 { "" } else { "s" },
                    if skipped == 1 { "was" } else { "were" },
                    task.task_id,
                    quoted.join(", "),
                    if skipped > quoted.len() { ", …" } else { "" },
                );
            }
            return answer;
        }
    }

    /// How many stray lines this pipe has skipped, over its life.
    pub fn stray_lines(&self) -> usize {
        self.strays
    }

    /// How many lines the reader thread has handed over, over the pipe's life.
    pub fn queued_lines(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    /// Wait for one line until `deadline` after `start` — the one place a task's
    /// deadline is divided between its write and its read.
    ///
    /// A timeout is a WORKER fault: the peer took the whole task and did not
    /// answer, so it is not usable, whatever the task's own merits.
    fn recv_line(&mut self, start: Instant, deadline: Duration) -> Result<Line, DispatchError> {
        match self
            .lines
            .recv_timeout(deadline.saturating_sub(start.elapsed()))
        {
            Ok(line) => Ok(line),
            // The whole deadline, not what was left of it for the read.
            Err(RecvTimeoutError::Timeout) => Err(DispatchError::Worker(format!(
                "worker did not answer within {:.1}s",
                deadline.as_secs_f64()
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(DispatchError::Worker(
                "worker closed stdout unexpectedly".to_string(),
            )),
        }
    }

    /// Ask the worker to exit, wait `SHUTDOWN_GRACE`, then kill it.
    ///
    /// Bounded on purpose: an unbounded `wait()` here hangs a run that has
    /// already produced every answer it was asked for (FND-137).
    pub fn shutdown(&mut self) -> std::io::Result<std::process::ExitStatus> {
        // One grace for the line and the exit together, so a handle whose writer
        // is stuck still keeps the promised bound.
        let start = Instant::now();
        let _ = self.send(r#"{"cmd":"shutdown"}"#, SHUTDOWN_GRACE);
        let deadline = start + SHUTDOWN_GRACE;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                let _ = self.child.kill();
                return self.child.wait();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The child's exit status, waiting up to `grace` for it to exit.
    ///
    /// For telling WHO closed the pipe after an EOF: stdout closes as the process
    /// exits, so the status is normally there at once; a child still running
    /// after `grace` reports `None`.
    pub fn exit_status_within(&mut self, grace: Duration) -> Option<std::process::ExitStatus> {
        let start = std::time::Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if start.elapsed() < grace => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => return None,
            }
        }
    }

    /// Kill the child. Closing its stdout is what ends the reader thread.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// A task line past any pipe buffer here: the default is 64 KiB and the most
    /// a child could raise it to is `/proc/sys/fs/pipe-max-size`, 1 MiB. Not on
    /// the worker's own 4 MiB line limit either.
    const LONG_LINE: usize = 2 * 1024 * 1024;

    fn pipe_to(script: &str) -> WorkerPipe {
        let child = Command::new("sh")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn sh");
        WorkerPipe::new(child)
    }

    /// FND-244: a child that never reads stdin. The write is bounded: it comes
    /// back within its deadline, a not-delivered `Unreachable`, poisons the pipe,
    /// and the pipe is still killable. Run on a thread with an outer bound, so
    /// a synchronous write fails this test instead of hanging the suite.
    #[test]
    fn a_child_that_never_reads_cannot_block_the_write() {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut pipe = pipe_to("exec sleep 300");
            let line = "x".repeat(LONG_LINE);
            let start = Instant::now();
            let first = pipe.send(&line, Duration::from_millis(500));
            let took = start.elapsed();
            let second = pipe.send("{}", Duration::from_secs(5));
            pipe.kill();
            let _ = tx.send((first, took, second));
        });
        let (first, took, second) = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the write to a child that never reads was not bounded");
        assert!(matches!(first, Err(SendError::Timeout(_))), "{first:?}");
        assert!(
            took < Duration::from_secs(3),
            "took {took:?} for a 0.5 s deadline"
        );
        assert!(matches!(second, Err(SendError::Poisoned)), "{second:?}");
        assert!(matches!(
            DispatchError::from(SendError::Timeout(Duration::from_secs(1))),
            DispatchError::Unreachable(_)
        ));
    }

    /// The control: a child that drains stdin takes the same line at once.
    #[test]
    fn a_child_that_drains_takes_a_long_line() {
        let mut pipe = pipe_to("exec cat > /dev/null");
        let line = "x".repeat(LONG_LINE);
        assert!(pipe.send(&line, Duration::from_secs(10)).is_ok());
        pipe.kill();
    }

    /// A child already gone: the write is a broken pipe — the one failure a
    /// reconnect may mend. Waited for first, or a short line lands in the pipe
    /// buffer before it exits and the write succeeds.
    #[test]
    fn a_write_to_an_exited_child_is_a_broken_pipe() {
        let mut pipe = pipe_to("exit 0");
        let _ = pipe.child.wait();
        let line = "x".repeat(LONG_LINE);
        let r = pipe.send(&line, Duration::from_secs(10));
        assert!(matches!(&r, Err(SendError::Broken(_))), "{r:?}");
        assert!(r.unwrap_err().reconnects());
    }

    fn task(id: &str, frequency_hz: f64) -> TaskMessage {
        TaskMessage {
            task_id: id.to_string(),
            deck_hash: "h".to_string(),
            deck_b64: String::new(),
            solver_config: crate::protocol::WorkerSolverConfig::default(),
            frequency_hz,
        }
    }

    /// A solved result line, as a worker writes it.
    fn ok_line(id: &str, frequency_hz: f64) -> String {
        serde_json::to_string(&TaskResult::Ok {
            task_id: id.to_string(),
            frequency_hz,
            impedance: crate::protocol::Impedance {
                re_ohm: 73.0,
                im_ohm: 42.5,
            },
            vswr_50: 2.0,
            feedpoint_current_mag: 0.01,
            feedpoint_current_phase_deg: -30.0,
            exec_used: "cpu".to_string(),
            mesh: crate::protocol::MESH.to_string(),
            warnings: Vec::new(),
        })
        .expect("serialise")
    }

    /// Poll until the reader has handed over `n` lines, at most 5 s.
    fn wait_until_queued(pipe: &WorkerPipe, n: usize) {
        let start = Instant::now();
        while pipe.queued_lines() < n {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "the child queued {} of {n} lines in 5 s",
                pipe.queued_lines()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A child that prints `lines`, one per line, then waits: the answer is read
    /// without a task being sent, so nothing about the child's input matters.
    fn printing(lines: &[String]) -> WorkerPipe {
        let mut script = String::new();
        for l in lines {
            script.push_str(&format!("printf '%s\\n' '{l}'; "));
        }
        script.push_str("exec sleep 300");
        pipe_to(&script)
    }

    /// FND-326 T1: a banner before the answer is skipped and counted, and the
    /// answer is still this task's. Without the skip loop the banner is read as
    /// the answer and fails the task.
    #[test]
    fn a_banner_before_the_answer_is_skipped_and_counted() {
        let mut pipe = printing(&["banner from .bashrc".to_string(), ok_line("t1", 14.2e6)]);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            Instant::now(),
            Duration::from_secs(20),
        );
        let strays = pipe.stray_lines();
        pipe.kill();
        let r = r.expect("the answer after a banner must be this task's");
        assert_eq!(r.task_id(), "t1");
        assert_eq!(strays, 1, "the banner is one stray line");
    }

    /// T1's control: the same answer with no banner reads no stray.
    #[test]
    fn an_answer_with_no_banner_reads_no_stray() {
        let mut pipe = printing(&[ok_line("t1", 14.2e6)]);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            Instant::now(),
            Duration::from_secs(20),
        );
        let strays = pipe.stray_lines();
        pipe.kill();
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(strays, 0);
    }

    /// A VALID result naming another task is a stray too — the case a filter of
    /// unparseable lines alone lets through as this task's answer.
    #[test]
    fn a_result_for_another_task_is_a_stray() {
        let mut pipe = printing(&[ok_line("stale", 1.0e6), ok_line("t1", 14.2e6)]);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            Instant::now(),
            Duration::from_secs(20),
        );
        let strays = pipe.stray_lines();
        pipe.kill();
        let r = r.expect("the stale line must be skipped");
        assert!(matches!(r, TaskResult::Ok { frequency_hz, .. } if frequency_hz == 14.2e6));
        assert_eq!(strays, 1);
    }

    /// The right id at the wrong frequency is the worker's bug, not a stray: a
    /// Task fault, so the number is never printed against the task's frequency.
    #[test]
    fn the_right_id_at_another_frequency_is_refused() {
        let mut pipe = printing(&[ok_line("t1", 13.5e6)]);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            Instant::now(),
            Duration::from_secs(20),
        );
        pipe.kill();
        match r {
            Err(DispatchError::Task(m)) => assert!(m.contains("13500000"), "{m}"),
            other => panic!("expected a Task fault, got {other:?}"),
        }
    }

    /// `"unknown"` answers the waiting task (the strictness control): an error
    /// for an unreadable task comes back at once, not after the deadline. A
    /// solved result for `"unknown"` is refused — no worker sends one.
    #[test]
    fn unknown_answers_the_waiting_task() {
        let err = r#"{"status":"error","task_id":"unknown","frequency_hz":0.0,"error_code":"parse_error","error_message":"x"}"#;
        let mut pipe = printing(&[err.to_string()]);
        let start = Instant::now();
        let r = pipe.answer_to(&task("t1", 14.2e6), "test", start, Duration::from_secs(20));
        let took = start.elapsed();
        pipe.kill();
        assert!(matches!(&r, Ok(TaskResult::Error { .. })), "{r:?}");
        assert!(took < Duration::from_secs(5), "took {took:?}");

        let mut pipe = printing(&[ok_line("unknown", 14.2e6)]);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            Instant::now(),
            Duration::from_secs(20),
        );
        pipe.kill();
        assert!(matches!(&r, Err(DispatchError::Task(_))), "{r:?}");
    }

    /// A line naming this task that is not a readable result is this task's
    /// unusable answer (FND-117), not a stray to wait past.
    #[test]
    fn an_unreadable_line_naming_the_task_is_a_task_fault() {
        let mut pipe = printing(&[r#"{"status":"ok","task_id":"t1"}"#.to_string()]);
        let start = Instant::now();
        let r = pipe.answer_to(&task("t1", 14.2e6), "test", start, Duration::from_secs(20));
        pipe.kill();
        assert!(
            matches!(&r, Err(DispatchError::Task(m)) if m.contains("unreadable")),
            "{r:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    /// The cap: a child that writes only junk is a Worker fault after
    /// MAX_STRAY_LINES lines, long before its deadline.
    #[test]
    fn a_worker_that_only_babbles_is_a_worker_fault() {
        let mut pipe = pipe_to("exec yes banner");
        let start = Instant::now();
        let r = pipe.answer_to(&task("t1", 14.2e6), "test", start, Duration::from_secs(30));
        let took = start.elapsed();
        let strays = pipe.stray_lines();
        pipe.kill();
        assert!(
            matches!(&r, Err(DispatchError::Worker(m)) if m.contains("none the answer")),
            "{r:?}"
        );
        assert_eq!(strays, MAX_STRAY_LINES);
        assert!(
            took < Duration::from_secs(10),
            "the cap fired, not the clock: {took:?}"
        );
    }

    /// The reader runs at most QUEUED_LINES ahead of the task reading it: a
    /// child writing without end, with nobody reading, has handed over no more
    /// than that. With an unbounded channel it hands over every line it writes.
    #[test]
    fn the_reader_queues_a_bounded_number_of_lines() {
        let mut pipe = pipe_to("exec yes banner");
        // Polled, not slept: a child that had not started writing would leave
        // the count at 0 and pass on any build.
        wait_until_queued(&pipe, QUEUED_LINES);
        // Settle: an unbounded reader keeps going past the bound.
        std::thread::sleep(Duration::from_millis(100));
        let queued = pipe.queued_lines();
        pipe.kill();
        assert_eq!(
            queued, QUEUED_LINES,
            "{queued} lines queued, bound {QUEUED_LINES}"
        );
    }

    /// Strays do not extend an expired deadline: ten stray lines already queued
    /// past a 100 ms deadline end the wait at the first, a Worker fault, even
    /// though an answer is queued behind them.
    #[test]
    fn strays_do_not_extend_an_expired_deadline() {
        let mut lines: Vec<String> = (0..10).map(|i| format!("junk {i}")).collect();
        lines.push(ok_line("t1", 14.2e6));
        let mut pipe = printing(&lines);
        // Every line queued before the wait starts (polled, not slept): with a
        // line missing, a build without the check also times out, on the gap.
        wait_until_queued(&pipe, lines.len());
        let start = Instant::now() - Duration::from_millis(200);
        let r = pipe.answer_to(
            &task("t1", 14.2e6),
            "test",
            start,
            Duration::from_millis(100),
        );
        let strays = pipe.stray_lines();
        pipe.kill();
        assert!(
            matches!(&r, Err(DispatchError::Worker(m)) if m.contains("did not answer")),
            "{r:?}"
        );
        assert_eq!(
            strays, 1,
            "the wait ends at the first stray past the deadline"
        );
    }

    /// The quote is bounded and escaped.
    #[test]
    fn a_quoted_stray_is_cut_and_escaped() {
        let q = quote(&format!("\u{1b}[31m{}", "é".repeat(200)));
        assert!(q.starts_with("\"\\u{1b}[31m"), "{q}");
        assert!(q.len() < QUOTE_BYTES + 16, "{}", q.len());
    }

    /// FND-331: an answer line past the protocol maximum is refused, not grown,
    /// and the next real line is still read — FND-096's worker-side test,
    /// mirrored on the controller's side. Without the bound the 5 MiB line comes
    /// back as text; without the drain its tail is read as the next line.
    #[test]
    fn an_over_long_answer_is_refused_and_the_next_line_is_still_read() {
        let over = MAX_LINE_BYTES as usize + MAX_LINE_BYTES as usize / 4;
        let mut pipe = pipe_to(&format!(
            "head -c {over} /dev/zero | tr '\\0' x; echo; echo good; exec sleep 300"
        ));
        let deadline = Duration::from_secs(60);
        let first = pipe.recv_line(Instant::now(), deadline);
        let second = pipe.recv_line(Instant::now(), deadline);
        pipe.kill();
        match &first {
            Ok(Line::TooLong) => {}
            Ok(Line::Text(l)) => panic!(
                "an over-long line was delivered as text ({} bytes)",
                l.len()
            ),
            Err(e) => panic!("an over-long line must arrive as TooLong, got {e:?}"),
        }
        assert!(
            matches!(&second, Ok(Line::Text(l)) if l == "good"),
            "the line after the over-long one must still be read: {second:?}"
        );
    }

    /// The control on the bound's other side: a line of exactly the maximum,
    /// newline included, is delivered whole — the bound refuses only what is
    /// past it.
    #[test]
    fn an_answer_at_the_maximum_is_delivered() {
        let text = MAX_LINE_BYTES as usize - 1;
        let mut pipe = pipe_to(&format!(
            "head -c {text} /dev/zero | tr '\\0' x; echo; exec sleep 300"
        ));
        let r = pipe.recv_line(Instant::now(), Duration::from_secs(60));
        pipe.kill();
        match r {
            Ok(Line::Text(l)) => assert_eq!(l.len(), text),
            other => panic!("a line at the maximum must be delivered: {other:?}"),
        }
    }

    /// Only a broken connection reconnects; a host that stopped reading has
    /// spent a deadline already (the SSH dispatch's rule, which no CI test can
    /// reach — every SSH test needs a host).
    #[test]
    fn only_a_broken_pipe_reconnects() {
        assert!(SendError::Broken(String::new()).reconnects());
        for e in [
            SendError::Timeout(Duration::from_secs(1)),
            SendError::Poisoned,
            SendError::WriterGone,
        ] {
            assert!(!e.reconnects(), "{e:?}");
        }
    }
}
