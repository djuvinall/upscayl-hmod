//! Running `upscayl-bin` and reading what it says.
//!
//! The engine's exit code is not its verdict (finding 10 in
//! `docs/results/engine-probe.md`): an unreadable input exits 0. A run succeeded only
//! when stderr has the success line, has no `Error:` line, and the output exists.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SUCCESS_LINE: &str = "Upscayled Successfully!";

/// What one run left behind.
#[derive(Debug)]
pub struct Report {
    pub elapsed: Duration,
    pub exit_code: Option<i32>,
    /// The last progress value seen, 0 to 100.
    pub progress: f32,
    pub stderr_tail: Vec<String>,
    /// Every `Error:` sentence the engine printed, in order.
    pub errors: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The engine reported an error; the text is its `Error:` line, cleaned.
    Engine(String),
    /// It ran past the budget and was killed.
    Budget(Duration),
    /// It could not be started at all.
    Spawn(String),
    /// The caller asked it to stop (a cancelled job).
    Cancelled,
    /// It said nothing wrong but produced no file.
    NoOutput,
}

/// One progress or error line out of stderr. The engine ends lines with `\r\n` and
/// prints `NN.NN%` while working, then a bare `100.00` (finding 11).
pub fn parse_progress(line: &str) -> Option<f32> {
    let t = line.trim().trim_end_matches('%');
    let (whole, frac) = t.split_once('.')?;
    if whole.is_empty() || whole.len() > 3 || frac.len() != 2 {
        return None;
    }
    if !whole
        .bytes()
        .chain(frac.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    t.parse::<f32>().ok().filter(|v| (0.0..=100.0).contains(v))
}

/// The engine's own error sentence, without its emoji and the `Error:` label.
pub fn error_text(line: &str) -> Option<String> {
    let i = line.find("Error:")?;
    Some(line[i + "Error:".len()..].trim().to_string())
}

/// Turns stderr lines into a 0-100 progress value.
///
/// On one image the engine's own percentages are the progress. In directory mode they
/// are not: every worker prints its own file's 0-100 interleaved with the others (see
/// `docs/results/engine-probe/raw/dir-mode.txt`), so the first file to finish would read
/// as 100 %. There a file counts once, when its success line or its `Error:` line
/// appears, and the percentages are dropped.
#[derive(Debug, Clone, Copy)]
pub struct Meter {
    files: Option<usize>,
    done: usize,
    value: f32,
}

impl Meter {
    pub fn single() -> Self {
        Meter {
            files: None,
            done: 0,
            value: 0.0,
        }
    }

    pub fn directory(files: usize) -> Self {
        Meter {
            files: Some(files.max(1)),
            done: 0,
            value: 0.0,
        }
    }

    /// Feed one trimmed line. Returns true when the line was a bare percentage, which
    /// callers drop from the kept stderr.
    pub fn feed(&mut self, line: &str) -> bool {
        let pct = parse_progress(line);
        match self.files {
            None => {
                if let Some(p) = pct {
                    self.value = p;
                }
            }
            Some(n) => {
                if pct.is_none() && (line.contains(SUCCESS_LINE) || error_text(line).is_some()) {
                    self.done = (self.done + 1).min(n);
                    self.value = self.done as f32 / n as f32 * 100.0;
                }
            }
        }
        pct.is_some()
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

/// Run the engine on one image. `on_progress` is called with each new progress value.
pub fn run(
    exe: &Path,
    args: &[String],
    output: &Path,
    budget: Duration,
    on_progress: &dyn Fn(f32),
    should_stop: &dyn Fn() -> bool,
) -> Result<Report, (Failure, Report)> {
    let (report, killed, said_success) =
        spawn(exe, args, Meter::single(), budget, on_progress, should_stop)?;
    if killed {
        if should_stop() {
            return Err((Failure::Cancelled, report));
        }
        return Err((Failure::Budget(budget), report));
    }
    if let Some(err) = report.errors.first() {
        return Err((Failure::Engine(err.clone()), report));
    }
    if !said_success || !output.is_file() {
        if let Some(code) = report.exit_code.filter(|c| *c != 0) {
            let tail = report.stderr_tail.join(" / ");
            return Err((
                Failure::Engine(format!("the engine exited with code {code}: {tail}")),
                report,
            ));
        }
        return Err((Failure::NoOutput, report));
    }
    Ok(report)
}

/// Run the engine in directory mode. Per-file errors do not fail the run -- the engine
/// carries on past an unreadable file (finding 12) -- so the caller reads
/// `Report::errors` and checks each expected output itself. Only a failure to start, or
/// running past the budget, is an `Err`.
pub fn run_directory(
    exe: &Path,
    args: &[String],
    files: usize,
    budget: Duration,
    on_progress: &dyn Fn(f32),
    should_stop: &dyn Fn() -> bool,
) -> Result<Report, (Failure, Report)> {
    let (report, killed, _) = spawn(
        exe,
        args,
        Meter::directory(files),
        budget,
        on_progress,
        should_stop,
    )?;
    if killed {
        if should_stop() {
            return Err((Failure::Cancelled, report));
        }
        return Err((Failure::Budget(budget), report));
    }
    Ok(report)
}

/// Spawn, read stderr, enforce the budget. Returns the report, whether it was killed,
/// and whether the success line appeared.
fn spawn(
    exe: &Path,
    args: &[String],
    meter: Meter,
    budget: Duration,
    on_progress: &dyn Fn(f32),
    should_stop: &dyn Fn() -> bool,
) -> Result<(Report, bool, bool), (Failure, Report)> {
    let start = Instant::now();
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        // The per-spawn secret is this module's alone (INTEROP.md, the handshake):
        // never hand it to a child.
        .env_remove("HDECK_MODULE_SECRET");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let empty = |elapsed| Report {
        elapsed,
        exit_code: None,
        progress: 0.0,
        stderr_tail: vec![],
        errors: vec![],
    };
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err((
                Failure::Spawn(format!("could not start {}: {e}", exe.display())),
                empty(start.elapsed()),
            ))
        }
    };

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let progress: Arc<Mutex<Meter>> = Arc::new(Mutex::new(meter));
    let reader = {
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let lines = Arc::clone(&lines);
        let progress = Arc::clone(&progress);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut pending = String::new();
            loop {
                match stderr.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                        while let Some(i) = pending.find(['\n', '\r']) {
                            let line: String = pending.drain(..=i).collect();
                            let line = line.trim().to_string();
                            if line.is_empty() {
                                continue;
                            }
                            let pct = progress.lock().expect("progress lock").feed(&line);
                            if !pct {
                                if let Ok(mut l) = lines.lock() {
                                    l.push(line);
                                }
                            }
                        }
                    }
                }
            }
            let rest = pending.trim().to_string();
            if !rest.is_empty() {
                if let Ok(mut l) = lines.lock() {
                    l.push(rest);
                }
            }
        })
    };

    let mut last_reported = -1.0_f32;
    let mut killed = false;
    let status = loop {
        let p = progress.lock().expect("progress lock").value();
        if p > last_reported {
            last_reported = p;
            on_progress(p);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if start.elapsed() > budget || should_stop() {
            let _ = child.kill();
            let _ = child.wait();
            killed = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = reader.join();

    let all = lines.lock().map(|l| l.clone()).unwrap_or_default();
    let report = Report {
        elapsed: start.elapsed(),
        exit_code: status.and_then(|s| s.code()),
        progress: progress.lock().expect("progress lock").value(),
        // GPU capability lines are noise in a refusal; keep what follows them.
        errors: all.iter().filter_map(|l| error_text(l)).collect(),
        stderr_tail: {
            let kept: Vec<String> = all
                .iter()
                .filter(|l| !l.starts_with('['))
                .cloned()
                .collect();
            kept[kept.len().saturating_sub(6)..].to_vec()
        },
    };
    let said_success = all.iter().any(|l| l.contains(SUCCESS_LINE));
    Ok((report, killed, said_success))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_parse_and_others_do_not() {
        assert_eq!(parse_progress("0.00%"), Some(0.0));
        assert_eq!(parse_progress("25.00%"), Some(25.0));
        assert_eq!(parse_progress("100.00"), Some(100.0));
        assert_eq!(parse_progress("  75.00%\r"), Some(75.0));
        for not in [
            "[0 NVIDIA GeForce RTX 5070 Ti]  queueC=2[8]",
            "🙌 Upscayled Successfully!",
            "1.5",
            "abc.de",
            "1000.00",
            "",
        ] {
            assert_eq!(parse_progress(not), None, "{not:?}");
        }
    }

    #[test]
    fn error_text_is_the_sentence_after_the_label() {
        assert_eq!(
            error_text("🚨 Error: Couldn't read the image 'x.jpg'!").as_deref(),
            Some("Couldn't read the image 'x.jpg'!")
        );
        assert_eq!(error_text("100.00"), None);
    }

    #[test]
    fn single_meter_follows_the_engine_percentages() {
        let mut m = Meter::single();
        assert!(m.feed("25.00%"));
        assert_eq!(m.value(), 25.0);
        assert!(!m.feed("🙌 Upscayled Successfully!"));
        assert!(m.feed("100.00"));
        assert_eq!(m.value(), 100.0);
    }

    #[test]
    fn directory_meter_counts_files_not_percentages() {
        // The interleaving recorded in docs/results/engine-probe/raw/dir-mode.txt,
        // three files: one unreadable, two upscaled.
        let mut m = Meter::directory(3);
        for line in ["0.00%", "25.00%", "0.00%", "50.00%", "25.00%", "75.00%"] {
            assert!(m.feed(line));
            assert_eq!(m.value(), 0.0, "{line}");
        }
        assert!(!m.feed("🚨 Error: Couldn't read the image 'corrupt.jpeg'!"));
        assert!((m.value() - 100.0 / 3.0).abs() < 0.01);
        for line in ["50.00%", "75.00%", "100.00"] {
            m.feed(line);
        }
        assert!((m.value() - 100.0 / 3.0).abs() < 0.01);
        m.feed("🙌 Upscayled Successfully!");
        assert!((m.value() - 200.0 / 3.0).abs() < 0.01);
        m.feed("100.00");
        m.feed("🙌 Upscayled Successfully!");
        assert_eq!(m.value(), 100.0);
        // A stray extra line never overshoots.
        m.feed("🙌 Upscayled Successfully!");
        assert_eq!(m.value(), 100.0);
    }
}
