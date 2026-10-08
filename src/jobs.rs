//! Time and processes for Lua (DESIGN.md, "Plugins: Neovim's shape, in Lua"):
//! `ranma.defer`, `ranma.every` and `ranma.cancel`, and `ranma.spawn` and
//! `ranma.kill`, Neovim's `jobstart` in ranma's terms.
//!
//! Nothing here runs Lua on a thread. A timer is a deadline the event loop
//! already waits on, so an idle ranma with no timers still never wakes. A
//! spawned process is read on threads of its own, and what it prints comes
//! back as events: its lines a batch at a time, never a call per byte.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::config::{Op, Runtime};
use crate::pane::AppEvent;

/// The shortest interval `ranma.every` takes: a plugin polling faster than
/// this is a plugin that should be event-driven instead.
pub const MIN_EVERY: Duration = Duration::from_millis(50);
/// Timers one configuration may hold at once.
pub const MAX_TIMERS: usize = 256;
/// Processes one configuration may have running at once.
pub const MAX_JOBS: usize = 64;
/// What a job keeps of its output for `on_exit`: stdout (when it is not
/// read line by line) and stderr, each. Past it the text is cut.
pub const MAX_CAPTURE: usize = 1 << 20;
/// How long a job's lines gather before they are handed over as one batch.
pub const LINE_BATCH: Duration = Duration::from_millis(50);

/// Timer and job ids are one sequence, process-wide: a job's last event can
/// arrive after a reload, and an id the new configuration reused would hand
/// it to the wrong callback.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct Timer {
    pub due: Instant,
    /// `Some` for `ranma.every`.
    pub every: Option<Duration>,
    pub f: Rc<RegistryKey>,
}

/// What Lua is called with as a job runs and ends.
#[derive(Debug, Clone, Default)]
struct Hooks {
    on_line: Option<Rc<RegistryKey>>,
    on_exit: Option<Rc<RegistryKey>>,
}

#[derive(Default)]
struct Inner {
    timers: BTreeMap<u64, Timer>,
    jobs: HashMap<u64, Hooks>,
    /// The process group of each job that started and has not ended.
    running: HashMap<u64, i32>,
}

/// A configuration that goes (a reload, ranma quitting) takes its processes
/// with it: their callbacks went with it, so nothing would hear them end.
impl Drop for Inner {
    fn drop(&mut self) {
        for pgid in self.running.values() {
            kill(*pgid, libc::SIGTERM);
        }
    }
}

/// The timers and jobs of one configuration, shared between its Lua functions
/// and the app. Dropped with the configuration on a reload.
#[derive(Clone, Default)]
pub struct Jobs(Rc<RefCell<Inner>>);

impl std::fmt::Debug for Jobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i = self.0.borrow();
        f.debug_struct("Jobs")
            .field("timers", &i.timers.len())
            .field("jobs", &i.jobs.len())
            .finish()
    }
}

/// One timer that came due, for the app to call.
pub struct Due {
    pub id: u64,
    pub f: Rc<RegistryKey>,
    pub repeating: bool,
}

impl Jobs {
    /// Add timers made while the configuration loaded.
    pub fn adopt(&self, timers: Vec<(u64, Timer)>) {
        self.0.borrow_mut().timers.extend(timers);
    }

    pub fn next_due(&self) -> Option<Instant> {
        self.0.borrow().timers.values().map(|t| t.due).min()
    }

    /// The timers due at `now`, in id order. A repeating one is moved to its
    /// next tick (from now, if it fell behind: missed ticks are not made up);
    /// a one-shot is gone.
    pub fn take_due(&self, now: Instant) -> Vec<Due> {
        let mut inner = self.0.borrow_mut();
        let ids: Vec<u64> = inner
            .timers
            .iter()
            .filter(|(_, t)| t.due <= now)
            .map(|(id, _)| *id)
            .collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let t = inner.timers.get_mut(&id).expect("listed above");
            out.push(Due {
                id,
                f: t.f.clone(),
                repeating: t.every.is_some(),
            });
            match t.every {
                Some(iv) => t.due = (t.due + iv).max(now + iv / 2),
                None => {
                    inner.timers.remove(&id);
                }
            }
        }
        out
    }

    pub fn cancel(&self, id: u64) -> bool {
        self.0.borrow_mut().timers.remove(&id).is_some()
    }

    pub fn timer_count(&self) -> usize {
        self.0.borrow().timers.len()
    }

    /// The function a job's lines go to, if it asked for them.
    pub fn on_line(&self, id: u64) -> Option<Rc<RegistryKey>> {
        self.0.borrow().jobs.get(&id)?.on_line.clone()
    }

    /// A job ended: its `on_exit`, if any, and it is forgotten. `None` for
    /// one this configuration did not start (it is from before a reload).
    pub fn finish(&self, id: u64) -> Option<Option<Rc<RegistryKey>>> {
        let mut inner = self.0.borrow_mut();
        inner.running.remove(&id);
        Some(inner.jobs.remove(&id)?.on_exit)
    }

    /// Start a job `ranma.spawn` queued (see [`start`]).
    pub fn start(&self, id: u64, spec: SpawnSpec, tx: Sender<AppEvent>) {
        if let Some(pgid) = start(id, spec, tx) {
            self.0.borrow_mut().running.insert(id, pgid);
        }
    }

    /// `ranma.kill`: SIGTERM to the job's process group, if it still runs.
    pub fn kill(&self, id: u64) {
        if let Some(pgid) = self.0.borrow().running.get(&id) {
            kill(*pgid, libc::SIGTERM);
        }
    }

    pub fn has_job(&self, id: u64) -> bool {
        self.0.borrow().jobs.contains_key(&id)
    }
}

/// What `ranma.spawn` asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    /// A program and its arguments, run directly; a string is given to
    /// `/bin/sh -c` as one argument.
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    /// Hand stdout over line by line (`on_line`) instead of keeping it.
    pub lines: bool,
}

/// What happens to a job, as the app hears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    Lines(Vec<String>),
    Exit(Exit),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exit {
    /// The exit status, if it exited rather than being killed.
    pub code: Option<i32>,
    /// The signal that ended it, if one did.
    pub signal: Option<i32>,
    /// Its whole stdout, unless it was read line by line.
    pub stdout: Option<String>,
    pub stderr: String,
    /// It could not start, or ran past its timeout.
    pub error: Option<String>,
}

fn err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

/// Timers made while the configuration loads wait here, in the builder, so a
/// plugin that fails takes its timers with it.
pub type PendingTimers = Vec<(u64, Timer)>;

/// Runs its argument on the builder's pending timers and says `true` while
/// the configuration loads; says `false` once it has loaded.
pub type WithPending = fn(&Lua, &mut dyn FnMut(&mut PendingTimers)) -> bool;

/// Install `defer`, `every`, `cancel`, `spawn` and `kill` into `ranma`.
/// While loading, timers go into the builder (through `pending`); after
/// that, straight into `jobs`.
pub fn install(lua: &Lua, ranma: &Table, jobs: &Jobs, pending: WithPending) -> mlua::Result<()> {
    let timer = move |jobs: Jobs, repeating: bool, who: &'static str| {
        move |lua: &Lua, (ms, f): (f64, Function)| {
            if !ms.is_finite() || ms < 0.0 {
                return Err(err(format!("{who}: milliseconds must be 0 or more")));
            }
            let d = Duration::from_secs_f64(ms / 1000.0);
            if repeating && d < MIN_EVERY {
                return Err(err(format!(
                    "{who}: every {ms} ms is too often (at least {} ms)",
                    MIN_EVERY.as_millis()
                )));
            }
            let t = Timer {
                due: Instant::now() + d,
                every: repeating.then_some(d),
                f: Rc::new(lua.create_registry_value(f)?),
            };
            let id = next_id();
            let mut t = Some(t);
            let mut full = false;
            let loading = pending(lua, &mut |p| {
                if p.len() >= MAX_TIMERS {
                    full = true;
                } else {
                    p.push((id, t.take().expect("once")));
                }
            });
            if !loading {
                if jobs.timer_count() >= MAX_TIMERS {
                    full = true;
                } else {
                    jobs.0
                        .borrow_mut()
                        .timers
                        .insert(id, t.take().expect("once"));
                }
            }
            if full {
                return Err(err(format!("{who}: more than {MAX_TIMERS} timers")));
            }
            Ok(id)
        }
    };
    ranma.set(
        "defer",
        lua.create_function(timer(jobs.clone(), false, "ranma.defer"))?,
    )?;
    ranma.set(
        "every",
        lua.create_function(timer(jobs.clone(), true, "ranma.every"))?,
    )?;
    {
        let jobs = jobs.clone();
        ranma.set(
            "cancel",
            lua.create_function(move |lua, id: u64| {
                let mut gone = false;
                let loading = pending(lua, &mut |p| {
                    let before = p.len();
                    p.retain(|(i, _)| *i != id);
                    gone = p.len() != before;
                });
                Ok(if loading { gone } else { jobs.cancel(id) })
            })?,
        )?;
    }
    {
        let jobs = jobs.clone();
        ranma.set(
            "spawn",
            lua.create_function(move |lua, (cmd, opts): (Value, Option<Table>)| {
                let (spec, hooks) = parse_spawn(lua, cmd, opts.as_ref())?;
                let mut rt = runtime(lua, "ranma.spawn")?;
                if jobs.0.borrow().jobs.len() >= MAX_JOBS {
                    return Err(err(format!(
                        "ranma.spawn: {MAX_JOBS} jobs are running already"
                    )));
                }
                let id = next_id();
                jobs.0.borrow_mut().jobs.insert(id, hooks);
                rt.ops.push(Op::Spawn(id, spec));
                Ok(id)
            })?,
        )?;
    }
    {
        let jobs = jobs.clone();
        ranma.set(
            "kill",
            lua.create_function(move |lua, id: u64| {
                let mut rt = runtime(lua, "ranma.kill")?;
                if !jobs.has_job(id) {
                    return Ok(false);
                }
                rt.ops.push(Op::Kill(id));
                Ok(true)
            })?,
        )?;
    }
    Ok(())
}

fn runtime<'a>(lua: &'a Lua, who: &str) -> mlua::Result<mlua::AppDataRefMut<'a, Runtime>> {
    lua.app_data_mut::<Runtime>().ok_or_else(|| {
        err(format!(
            "{who} only works inside binds, hooks, modules and timers, not at config load"
        ))
    })
}

fn parse_spawn(lua: &Lua, cmd: Value, opts: Option<&Table>) -> mlua::Result<(SpawnSpec, Hooks)> {
    let argv = match cmd {
        Value::String(s) => vec!["/bin/sh".into(), "-c".into(), s.to_str()?.to_string()],
        Value::Table(t) => {
            let argv: Vec<String> = t
                .sequence_values::<String>()
                .collect::<mlua::Result<_>>()
                .map_err(|e| err(format!("ranma.spawn: the command's words: {e}")))?;
            if argv.is_empty() {
                return Err(err("ranma.spawn: an empty command"));
            }
            argv
        }
        other => {
            return Err(err(format!(
                "ranma.spawn: the command must be a string or a list of words, not {}",
                other.type_name()
            )));
        }
    };
    let mut spec = SpawnSpec {
        argv,
        cwd: None,
        timeout: None,
        lines: false,
    };
    let mut hooks = Hooks::default();
    if let Some(t) = opts {
        for pair in t.pairs::<String, Value>() {
            let (k, v) = pair?;
            match (k.as_str(), v) {
                ("cwd", Value::String(s)) => spec.cwd = Some(PathBuf::from(s.to_str()?.as_ref())),
                ("timeout", Value::Integer(n)) if n > 0 => {
                    spec.timeout = Some(Duration::from_secs(n as u64));
                }
                ("timeout", Value::Number(n)) if n > 0.0 && n.is_finite() => {
                    spec.timeout = Some(Duration::from_secs_f64(n));
                }
                ("on_exit", Value::Function(f)) => {
                    hooks.on_exit = Some(Rc::new(lua.create_registry_value(f)?));
                }
                ("on_line", Value::Function(f)) => {
                    hooks.on_line = Some(Rc::new(lua.create_registry_value(f)?));
                    spec.lines = true;
                }
                ("cwd" | "timeout" | "on_exit" | "on_line", other) => {
                    let want = match k.as_str() {
                        "cwd" => "a string",
                        "timeout" => "seconds, more than 0",
                        _ => "a function",
                    };
                    return Err(err(format!(
                        "ranma.spawn: `{k}` must be {want}, not {}",
                        other.type_name()
                    )));
                }
                _ => {
                    return Err(err(format!(
                        "ranma.spawn: unknown option `{k}` (expected cwd, timeout, on_line, on_exit)"
                    )));
                }
            }
        }
    }
    Ok((spec, hooks))
}

/// Start a job's process and the threads that read it. Returns its process
/// group, which `ranma.kill` and a reload signal. A process that cannot start
/// is not an error here: it arrives as an exit carrying the reason, like any
/// other end.
pub fn start(id: u64, spec: SpawnSpec, tx: Sender<AppEvent>) -> Option<i32> {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Command, Stdio};

    let send = move |tx: &Sender<AppEvent>, event| {
        let _ = tx.send(AppEvent::Job { id, event });
    };
    let mut cmd = Command::new(&spec.argv[0]);
    cmd.args(&spec.argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(d) = &spec.cwd {
        cmd.current_dir(d);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            send(
                &tx,
                JobEvent::Exit(Exit {
                    error: Some(format!("could not run `{}`: {e}", spec.argv[0])),
                    ..Exit::default()
                }),
            );
            return None;
        }
    };
    let pgid = child.id() as i32;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let err_reader = std::thread::spawn(move || read_capped(stderr));
    let timed_out = spec.timeout.map(|t| watchdog(pgid, t));

    let lines_tx = tx.clone();
    let out_reader = std::thread::spawn(move || {
        if spec.lines {
            forward_lines(stdout, |batch| send(&lines_tx, JobEvent::Lines(batch)));
            None
        } else {
            Some(read_capped(stdout))
        }
    });
    std::thread::Builder::new()
        .name(format!("job {id}"))
        .spawn(move || {
            let stdout = out_reader.join().unwrap_or(None);
            let stderr = err_reader.join().unwrap_or_default();
            let status = child.wait();
            let late = timed_out.as_ref().is_some_and(|(done, late)| {
                done.store(true, Ordering::Release);
                late.load(Ordering::Acquire)
            });
            let mut exit = Exit {
                stdout,
                stderr,
                ..Exit::default()
            };
            match status {
                Ok(s) => {
                    exit.code = s.code();
                    exit.signal = s.signal();
                }
                Err(e) => exit.error = Some(format!("waiting for it: {e}")),
            }
            if late {
                exit.error = Some(format!(
                    "timed out after {}s",
                    spec.timeout.unwrap_or_default().as_secs_f64()
                ));
            }
            send(&tx, JobEvent::Exit(exit));
        })
        .expect("spawning a job thread");
    Some(pgid)
}

/// Kill the group after `timeout` unless told it is done; the second flag
/// says it had to.
fn watchdog(
    pgid: i32,
    timeout: Duration,
) -> (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let done = Arc::new(AtomicBool::new(false));
    let late = Arc::new(AtomicBool::new(false));
    let (d, l) = (done.clone(), late.clone());
    std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if d.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if !d.load(Ordering::Acquire) {
            l.store(true, Ordering::Release);
            kill(pgid, libc::SIGKILL);
        }
    });
    (done, late)
}

/// Signal a job's whole process group: killing only its first process would
/// leave anything it started holding the output pipe open.
pub fn kill(pgid: i32, signal: i32) {
    // SAFETY: kill(2) with a negative pid signals a process group this
    // process created; no memory is involved.
    unsafe {
        libc::kill(-pgid, signal);
    }
}

fn read_capped(mut r: impl Read) -> String {
    let mut buf = Vec::new();
    let _ = (&mut r).take(MAX_CAPTURE as u64).read_to_end(&mut buf);
    // Drain the rest, so the process is not blocked on a full pipe.
    let _ = std::io::copy(&mut r, &mut std::io::sink());
    String::from_utf8_lossy(&buf).into_owned()
}

/// Read lines and hand them over in batches: whatever arrived within
/// [`LINE_BATCH`] of the first line of a batch goes in one.
fn forward_lines(r: impl Read + Send + 'static, mut send: impl FnMut(Vec<String>)) {
    let (ltx, lrx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(r).lines() {
            let Ok(line) = line else { break };
            if ltx.send(line).is_err() {
                break;
            }
        }
    });
    while let Ok(first) = lrx.recv() {
        let mut batch = vec![first];
        let until = Instant::now() + LINE_BATCH;
        loop {
            match lrx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(l) => batch.push(l),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    send(std::mem::take(&mut batch));
                    return;
                }
            }
        }
        send(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_from;
    use std::sync::mpsc::channel;

    fn events(id: u64, rx: &std::sync::mpsc::Receiver<AppEvent>) -> Vec<JobEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.recv_timeout(Duration::from_secs(5)) {
            let AppEvent::Job { id: got, event } = ev else {
                continue;
            };
            assert_eq!(got, id);
            let end = matches!(event, JobEvent::Exit(_));
            out.push(event);
            if end {
                break;
            }
        }
        out
    }

    fn spec(argv: &[&str], lines: bool) -> SpawnSpec {
        SpawnSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            timeout: None,
            lines,
        }
    }

    #[test]
    fn a_job_reports_its_output_and_status() {
        let (tx, rx) = channel();
        start(
            1,
            spec(&["sh", "-c", "echo out; echo err >&2; exit 3"], false),
            tx,
        )
        .unwrap();
        let ev = events(1, &rx);
        assert_eq!(
            ev,
            [JobEvent::Exit(Exit {
                code: Some(3),
                stdout: Some("out\n".into()),
                stderr: "err\n".into(),
                ..Exit::default()
            })]
        );
    }

    #[test]
    fn lines_arrive_in_batches_before_the_exit() {
        let (tx, rx) = channel();
        start(
            2,
            spec(&["sh", "-c", "printf 'a\\nb\\n'; sleep 0.3; echo c"], true),
            tx,
        )
        .unwrap();
        let ev = events(2, &rx);
        assert_eq!(ev[0], JobEvent::Lines(vec!["a".into(), "b".into()]));
        assert_eq!(ev[1], JobEvent::Lines(vec!["c".into()]));
        let JobEvent::Exit(e) = &ev[2] else { panic!() };
        assert_eq!((e.code, e.stdout.as_deref()), (Some(0), None));
    }

    #[test]
    fn a_job_that_cannot_start_or_overruns_says_why() {
        let (tx, rx) = channel();
        assert_eq!(
            start(3, spec(&["/no/such/program"], false), tx.clone()),
            None
        );
        let JobEvent::Exit(e) = &events(3, &rx)[0] else {
            panic!()
        };
        assert!(e.error.as_ref().unwrap().contains("could not run"));

        let mut s = spec(&["sh", "-c", "sleep 1 & sleep 30"], false);
        s.timeout = Some(Duration::from_millis(200));
        let started = Instant::now();
        start(4, s, tx).unwrap();
        let JobEvent::Exit(e) = &events(4, &rx)[0] else {
            panic!()
        };
        assert!(e.error.as_ref().unwrap().contains("timed out"), "{e:?}");
        assert_eq!(e.signal, Some(libc::SIGKILL));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the whole group died"
        );
    }

    #[test]
    fn timers_come_due_in_order_and_repeat_without_making_up_ticks() {
        let cfg = load_from(None, None, None).unwrap();
        let f = || {
            Rc::new(
                cfg.lua
                    .create_registry_value(cfg.lua.create_function(|_, ()| Ok(())).unwrap())
                    .unwrap(),
            )
        };
        let jobs = Jobs::default();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        jobs.adopt(vec![
            (
                10,
                Timer {
                    due: at(100),
                    every: None,
                    f: f(),
                },
            ),
            (
                11,
                Timer {
                    due: at(50),
                    every: Some(Duration::from_millis(100)),
                    f: f(),
                },
            ),
        ]);
        assert_eq!(jobs.next_due(), Some(at(50)));
        assert!(jobs.take_due(at(10)).is_empty());
        let due: Vec<(u64, bool)> = jobs
            .take_due(at(120))
            .iter()
            .map(|d| (d.id, d.repeating))
            .collect();
        assert_eq!(due, [(10, false), (11, true)]);
        assert_eq!(jobs.timer_count(), 1, "the one-shot is gone");
        assert_eq!(jobs.next_due(), Some(at(170)), "120 + half the interval");
        // A long stall: one tick, not ten.
        assert_eq!(jobs.take_due(at(1200)).len(), 1);
        assert!(jobs.cancel(11) && !jobs.cancel(11));
        assert_eq!(jobs.next_due(), None);
    }
}
