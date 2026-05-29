//! MiniCPM5 local-model onboarding: detect whether to recommend the on-device
//! model, and run its installer with streamed progress.
//!
//! The detection + install logic lives in `scripts/minicpm5-{recommend,install}.sh`
//! (single source of truth, also usable from a terminal). These commands just
//! locate and run them, surfacing the result to the desktop onboarding card.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// Locate a repo `scripts/<name>` from a dev/bundled layout. Mirrors
/// daemon_launcher's resources-dir walk: climb from the exe (and cwd) looking
/// for a `scripts/` sibling of the bundled `resources/`.
fn script_path(name: &str) -> Option<PathBuf> {
    if let Ok(repo) = std::env::var("PUFFER_REPO") {
        let p = PathBuf::from(repo).join("scripts").join(name);
        if p.exists() {
            return Some(p);
        }
    }
    let bases = [
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf())),
        std::env::current_dir().ok(),
    ];
    for base in bases.into_iter().flatten() {
        let mut dir = base;
        for _ in 0..8 {
            let candidate = dir.join("scripts").join(name);
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    None
}

/// Should puffer recommend installing the local model on this machine? Returns
/// the recommend.sh JSON decision (recommend/false + reason/metadata).
#[tauri::command]
pub fn minicpm5_recommend() -> Value {
    let Some(script) = script_path("minicpm5-recommend.sh") else {
        return json!({ "recommend": false, "reason": "installer scripts not found" });
    };
    match Command::new("/bin/bash").arg(&script).output() {
        Ok(out) => {
            let txt = String::from_utf8_lossy(&out.stdout);
            let last = txt.trim().lines().last().unwrap_or("{}");
            serde_json::from_str(last)
                .unwrap_or_else(|_| json!({ "recommend": false, "reason": "decision parse failed" }))
        }
        Err(err) => json!({ "recommend": false, "reason": format!("run failed: {err}") }),
    }
}

/// Run the installer in the background, streaming stdout/stderr lines as
/// `minicpm5://install-log` events and a final `minicpm5://install-done`
/// ({ success: bool }). Non-blocking so the UI stays responsive during the
/// multi-minute weight download.
#[tauri::command]
pub fn minicpm5_install(app: AppHandle) -> Result<(), String> {
    let script = script_path("minicpm5-install.sh").ok_or("installer script not found")?;
    std::thread::spawn(move || {
        let spawned = Command::new("/bin/bash")
            .arg(&script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(err) => {
                let _ = app.emit(
                    "minicpm5://install-done",
                    json!({ "success": false, "error": err.to_string() }),
                );
                return;
            }
        };

        // Merge stderr into the same log stream so progress + warnings show.
        if let Some(err) = child.stderr.take() {
            let app = app.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = app.emit("minicpm5://install-log", line);
                }
            });
        }
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = app.emit("minicpm5://install-log", line);
            }
        }

        let success = child.wait().map(|s| s.success()).unwrap_or(false);
        let _ = app.emit("minicpm5://install-done", json!({ "success": success }));
    });
    Ok(())
}
