//! `leader U` in the background: the update runs out of sight, and a toast
//! says how it goes, with a spinner while it works (DESIGN.md, "An update
//! is a toast, not a pane").
//!
//! The job itself is `update::JobFiles`: it reports through files, because
//! `install.sh` exec's this server into the new build halfway through. The
//! server that comes back finds the job on record under its name and picks
//! the toast up again (`resume_update`).

use std::time::{Duration, Instant};

use super::App;
use crate::toast::Level;
use crate::update::{JobFiles, Progress};

/// A spinner frame. Only while a job runs, so an idle ranma stays idle.
const FRAME: Duration = Duration::from_millis(120);
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// How long the result stays: long enough to read after a coffee, not forever.
const RESULT_FOR: Duration = Duration::from_secs(15);

pub(super) struct UpdateJob {
    pid: u32,
    files: JobFiles,
    toast: u64,
    started: Instant,
    frame: usize,
    next: Instant,
}

impl App {
    /// `leader U` on a ranma that updates itself (not a packaged one).
    pub(super) fn start_update(&mut self) {
        if self.update_job.is_some() {
            self.status = Some("ranma is already updating".into());
            return;
        }
        let (source, files) = match (crate::update::Source::current(), JobFiles::new()) {
            (Ok(s), Some(f)) => (s, f),
            (Err(e), _) => {
                self.status = Some(format!("update failed: {e:#}"));
                return;
            }
            (_, None) => {
                self.status = Some("update failed: no cache directory".into());
                return;
            }
        };
        let server = self.job_owner();
        match files.start(&source, &server) {
            Ok(pid) => {
                self.update_available = None;
                self.track_update(pid, files, Instant::now());
            }
            Err(e) => self.status = Some(format!("update failed: {e:#}")),
        }
    }

    /// After a start or an exec: a job this server started is shown again,
    /// still running or finished while the server was being replaced.
    pub fn resume_update(&mut self) {
        let Some(files) = JobFiles::new() else {
            return;
        };
        let Some(pid) = files.owned_by(&self.job_owner()) else {
            return;
        };
        match files.progress() {
            Progress::Running(_) if !crate::update::alive(pid) => {
                files.clear();
                self.update_failed(&files, "the update stopped without saying how it went");
            }
            _ => {
                // The time it started is not on record; the spinner's clock
                // starts again, which only shows less than it took.
                self.track_update(pid, files, Instant::now());
                // The upgrade's own "ranma upgraded to" toast says less than
                // the job will; this one replaces it.
                self.toasts.dismiss_text_prefix("ranma upgraded to ");
                self.update_tick(Instant::now());
            }
        }
    }

    fn track_update(&mut self, pid: u32, files: JobFiles, now: Instant) {
        let toast = self
            .toasts
            .push_sticky(spinner_text(0, "", Duration::ZERO), now);
        self.update_job = Some(UpdateJob {
            pid,
            files,
            toast,
            started: now,
            frame: 0,
            next: now + FRAME,
        });
        self.dirty = true;
    }

    /// The server's name as a job records it; a standalone ranma is not
    /// exec'd by an upgrade, so its pid is name enough.
    fn job_owner(&self) -> String {
        self.server_name
            .clone()
            .unwrap_or_else(|| format!("standalone-{}", std::process::id()))
    }

    pub(super) fn update_due(&self) -> Option<Instant> {
        self.update_job.as_ref().map(|j| j.next)
    }

    pub(super) fn update_tick(&mut self, now: Instant) {
        let Some(job) = self.update_job.as_mut() else {
            return;
        };
        if now < job.next {
            return;
        }
        match job.files.progress() {
            Progress::Running(phase) => {
                job.frame = job.frame.wrapping_add(1);
                job.next = now + FRAME;
                let text = spinner_text(job.frame, &phase, now - job.started);
                if !self.toasts.set(
                    job.toast,
                    text.clone(),
                    Level::Normal,
                    Duration::from_secs(86_400),
                    now,
                ) {
                    // Dismissed with a click: the job goes on, and so does
                    // the toast, since it is the only sign of it.
                    job.toast = self.toasts.push_sticky(text, now);
                }
                self.dirty = true;
            }
            Progress::Done(code) => {
                let Some(job) = self.update_job.take() else {
                    return;
                };
                crate::update::reap(job.pid);
                let installed = job.files.installed();
                job.files.clear();
                self.toasts.dismiss(job.toast);
                if code == 0 {
                    // The commit the job installed, not this binary's: a
                    // standalone ranma is not exec'd, so it still runs the old.
                    let what = installed.map_or(String::new(), |c| format!(" to {c}"));
                    let text = if self.server_name.is_some() {
                        format!("ranma updated{what}")
                    } else {
                        format!("ranma updated{what} · restart ranma to use it")
                    };
                    self.toast(text, Level::Normal, Some(RESULT_FOR));
                } else {
                    self.update_failed(&job.files, &format!("exit {code}"));
                }
            }
        }
    }

    fn update_failed(&mut self, files: &JobFiles, why: &str) {
        let now = Instant::now();
        self.toasts.push(
            format!("ranma update failed ({why}); nothing was replaced · click for the log"),
            Level::Urgent,
            RESULT_FOR * 2,
            now,
        );
        self.update_log = Some((self.toasts.last_id(), files.log()));
        self.dirty = true;
    }

    /// A click on a toast: the failed update's opens its log in a float.
    pub(super) fn update_toast_clicked(&mut self, id: u64) {
        let Some((_, log)) = self.update_log.take_if(|(t, _)| *t == id) else {
            return;
        };
        let path = log.to_string_lossy().replace('\'', "'\\''");
        let cmd = format!("less -R +G '{path}'");
        match self.open_pane_at(Some(&cmd), None, None) {
            Ok(pane) => {
                if !self.scratch_shown {
                    self.toggle_floating_pane(pane);
                }
                self.rename_pane(pane, "ranma update log");
            }
            Err(e) => self.status = Some(format!("opening the log: {e:#}")),
        }
    }
}

fn spinner_text(frame: usize, phase: &str, took: Duration) -> String {
    let what = match phase {
        "clone" => "fetching the source",
        "pull" => "pulling",
        "build" => "building",
        _ => "starting",
    };
    let s = took.as_secs();
    format!(
        "{} Updating ranma… {what} {}:{:02}",
        SPINNER[frame % SPINNER.len()],
        s / 60,
        s % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spinner_says_the_step_and_the_time() {
        assert_eq!(
            spinner_text(0, "build", Duration::from_secs(83)),
            "⠋ Updating ranma… building 1:23"
        );
        assert_eq!(
            spinner_text(11, "pull", Duration::ZERO),
            "⠙ Updating ranma… pulling 0:00"
        );
    }
}
