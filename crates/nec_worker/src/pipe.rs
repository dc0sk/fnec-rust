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
//! which ends the thread's `read_line` at EOF.
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

use std::io::{BufRead, BufReader, Write};
use std::process::Child;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

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
    lines: Receiver<String>,
    poisoned: bool,
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
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    // A closed receiver means the handle is gone; stop reading.
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            child,
            jobs,
            lines,
            poisoned: false,
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

    /// One task's round trip: write `line` and wait for the answer, both within
    /// one `deadline` — the read gets what the write left.
    pub fn exchange(&mut self, line: &str, deadline: Duration) -> Result<String, DispatchError> {
        let start = Instant::now();
        self.send(line, deadline)?;
        self.recv_by(start, deadline)
    }

    /// Wait for one line until `deadline` after `start` — the one place a task's
    /// deadline is divided between its write and its read.
    ///
    /// A timeout is a WORKER fault: the peer took the whole task and did not
    /// answer, so it is not usable, whatever the task's own merits.
    pub fn recv_by(&mut self, start: Instant, deadline: Duration) -> Result<String, DispatchError> {
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
