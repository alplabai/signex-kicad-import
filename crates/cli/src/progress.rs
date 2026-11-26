// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! NDJSON progress events on stdout (contract section 3.1).
//!
//! One JSON object per line, flushed per line. A heartbeat thread repeats
//! the last progress event when nothing else was printed for the interval,
//! so a long item never leaves the reader silent; the contract allows 10 s
//! and the default here is 5 s. The summary is always the last line:
//! `finish` stops the heartbeat before it writes it.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Silence the heartbeat breaks (the contract's bound is 10 s).
pub const HEARTBEAT: Duration = Duration::from_secs(5);

struct Inner {
    out: Box<dyn Write + Send>,
    last_progress: Option<String>,
    last_emit: Instant,
}

impl Inner {
    fn line(&mut self, text: &str) {
        // A closed stdout is the reader's choice; the run goes on.
        let _ = writeln!(self.out, "{text}");
        let _ = self.out.flush();
        self.last_emit = Instant::now();
    }
}

pub struct Emitter {
    inner: Option<Arc<Mutex<Inner>>>,
    stop: Arc<AtomicBool>,
    beat: Option<JoinHandle<()>>,
}

impl Emitter {
    /// An emitter that prints nothing.
    pub fn off() -> Self {
        Emitter {
            inner: None,
            stop: Arc::new(AtomicBool::new(false)),
            beat: None,
        }
    }

    pub fn on(out: Box<dyn Write + Send>, heartbeat: Duration) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            out,
            last_progress: None,
            last_emit: Instant::now(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let tick = (heartbeat / 4).clamp(Duration::from_millis(5), Duration::from_millis(250));
        let beat = {
            let inner = Arc::clone(&inner);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(tick);
                    let Ok(mut guard) = inner.lock() else { return };
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if guard.last_emit.elapsed() >= heartbeat
                        && let Some(line) = guard.last_progress.clone()
                    {
                        guard.line(&line);
                    }
                }
            })
        };
        Emitter {
            inner: Some(inner),
            stop,
            beat: Some(beat),
        }
    }

    fn emit(&self, event: &Value, remember: Option<String>) {
        let Some(inner) = &self.inner else { return };
        let Ok(mut guard) = inner.lock() else { return };
        guard.line(&event.to_string());
        if remember.is_some() {
            guard.last_progress = remember;
        }
    }

    /// A new phase starts with `total` items, `None` when unknown.
    pub fn phase(&self, phase: &str, total: Option<usize>) {
        self.emit(
            &json!({ "event": "phase", "phase": phase, "total": total }),
            Some(progress_line(0, total, None)),
        );
    }

    pub fn progress(&self, done: usize, total: Option<usize>, item: &str) {
        let line = progress_line(done, total, Some(item));
        self.emit(
            &serde_json::from_str(&line).unwrap_or(Value::Null),
            Some(line),
        );
    }

    /// Stop the heartbeat and print the summary as the last line.
    pub fn finish(mut self, summary: &Value) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(beat) = self.beat.take() {
            let _ = beat.join();
        }
        self.emit(summary, None);
    }
}

impl Drop for Emitter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(beat) = self.beat.take() {
            let _ = beat.join();
        }
    }
}

fn progress_line(done: usize, total: Option<usize>, item: Option<&str>) -> String {
    json!({ "event": "progress", "done": done, "total": total, "item": item }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("sink").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn lines(sink: &Sink) -> Vec<Value> {
        let text = String::from_utf8(sink.0.lock().expect("sink").clone()).expect("utf8");
        text.lines()
            .map(|l| serde_json::from_str(l).expect("a JSON line"))
            .collect()
    }

    #[test]
    fn the_summary_is_the_last_line() {
        let sink = Sink::default();
        let emitter = Emitter::on(Box::new(sink.clone()), Duration::from_millis(20));
        emitter.phase("read", Some(2));
        emitter.progress(1, Some(2), "a");
        emitter.finish(&json!({ "event": "summary", "ok": true }));
        let events = lines(&sink);
        assert_eq!(events[0]["event"], "phase");
        assert_eq!(events[1]["item"], "a");
        assert_eq!(events.last().expect("a line")["event"], "summary");
        assert_eq!(events.iter().filter(|e| e["event"] == "summary").count(), 1);
    }

    #[test]
    fn a_silent_stretch_repeats_the_last_progress() {
        let sink = Sink::default();
        let emitter = Emitter::on(Box::new(sink.clone()), Duration::from_millis(40));
        emitter.phase("read", Some(5));
        emitter.progress(2, Some(5), "slow");
        std::thread::sleep(Duration::from_millis(300));
        emitter.finish(&json!({ "event": "summary" }));
        let repeats = lines(&sink)
            .iter()
            .filter(|e| e["event"] == "progress" && e["item"] == "slow")
            .count();
        assert!(repeats >= 2, "heartbeat repeated {repeats} times");
    }

    #[test]
    fn an_off_emitter_prints_nothing_and_does_not_panic() {
        let emitter = Emitter::off();
        emitter.phase("read", None);
        emitter.progress(0, None, "x");
        emitter.finish(&json!({}));
    }
}
