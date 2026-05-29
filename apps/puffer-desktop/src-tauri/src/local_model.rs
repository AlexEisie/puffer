use crate::events::EventEmitter;
use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::json;
use std::env;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

const MINICPM5_ID: &str = "minicpm5";
const MINICPM5_MODEL_ID: &str = "minicpm5-1b";
const MINICPM5_DISPLAY_NAME: &str = "MiniCPM5-1B (local)";
const MINICPM5_REPO: &str = "openbmb/MiniCPM5-1B-MLX";
const MINICPM5_ENDPOINT: &str = "http://127.0.0.1:8088/v1";
const MINICPM5_MODELS_URL: &str = "http://127.0.0.1:8088/v1/models";
const MINICPM5_EVENT: &str = "local-model:minicpm5:event";
const MINICPM5_SIZE: &str = "~589MB";
const MINICPM5_SHIM: &str = include_str!("../../../../scripts/minicpm5_shim.py");
const MINICPM5_PROVIDER: &str = include_str!("../../../../resources/providers/minicpm5.yaml");

#[derive(Clone)]
pub(crate) struct LocalModelInstaller {
    installing: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelStatus {
    id: String,
    model_id: String,
    display_name: String,
    supported: bool,
    recommended: bool,
    installed: bool,
    configured: bool,
    running: bool,
    installing: bool,
    reason: String,
    endpoint: String,
    size: String,
    install_path: String,
    provider_path: String,
    log_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelInstallJob {
    job_id: String,
    status: LocalModelStatus,
}

struct MiniCpm5Paths {
    puffer_home: PathBuf,
    venv: PathBuf,
    python: PathBuf,
    model_dir: PathBuf,
    model_config: PathBuf,
    bin_dir: PathBuf,
    shim: PathBuf,
    provider: PathBuf,
    install_log: PathBuf,
    serve_log: PathBuf,
}

impl LocalModelInstaller {
    /// Creates a local-model installer with in-process job de-duplication.
    pub(crate) fn new() -> Self {
        Self {
            installing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the current installation and server status for a local model.
    pub(crate) fn status(&self, model_id: &str) -> Result<LocalModelStatus> {
        ensure_minicpm5(model_id)?;
        Ok(build_status(self.installing.load(Ordering::SeqCst)))
    }

    /// Starts the one-click MiniCPM5 installer or returns the active job state.
    pub(crate) fn install_or_start(
        &self,
        events: EventEmitter,
        model_id: &str,
    ) -> Result<LocalModelInstallJob> {
        ensure_minicpm5(model_id)?;
        if self
            .installing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(LocalModelInstallJob {
                job_id: "active".to_string(),
                status: build_status(true),
            });
        }

        let job_id = Uuid::new_v4().to_string();
        let installer = self.clone();
        let thread_job_id = job_id.clone();
        thread::spawn(move || {
            let result = installer.run_install_job(&events, &thread_job_id);
            if let Err(error) = result {
                emit_model_event(
                    &events,
                    &thread_job_id,
                    "error",
                    format!("{error:#}"),
                    Some(build_status(false)),
                );
            }
            installer.installing.store(false, Ordering::SeqCst);
        });

        Ok(LocalModelInstallJob {
            job_id,
            status: build_status(true),
        })
    }

    fn run_install_job(&self, events: &EventEmitter, job_id: &str) -> Result<()> {
        let paths = minicpm5_paths();
        let status = build_status(true);
        if !status.supported {
            bail!(status.reason);
        }

        fs::create_dir_all(&paths.puffer_home).with_context(|| {
            format!(
                "failed to create Puffer home {}",
                paths.puffer_home.display()
            )
        })?;
        reset_log(&paths.install_log)?;
        emit_model_event(
            events,
            job_id,
            "checking",
            "Checking local MiniCPM5 runtime".to_string(),
            Some(status),
        );

        if !paths.python.exists() {
            emit_model_event(
                events,
                job_id,
                "runtime",
                format!("Creating isolated Python venv at {}", paths.venv.display()),
                None,
            );
            run_logged(
                "runtime",
                "python3",
                &["-m", "venv", path_str(&paths.venv)?],
                &[],
                &paths.install_log,
            )?;
        }

        if !python_imports(&paths.python, "mlx_lm")
            || !python_imports(&paths.python, "huggingface_hub")
        {
            emit_model_event(
                events,
                job_id,
                "runtime",
                "Installing mlx-lm and huggingface_hub into the isolated venv".to_string(),
                None,
            );
            run_logged(
                "pip-upgrade",
                path_str(&paths.python)?,
                &["-m", "pip", "install", "--quiet", "--upgrade", "pip"],
                &[],
                &paths.install_log,
            )?;
            run_logged(
                "pip-install",
                path_str(&paths.python)?,
                &[
                    "-m",
                    "pip",
                    "install",
                    "--quiet",
                    "--upgrade",
                    "mlx-lm",
                    "huggingface_hub",
                ],
                &[],
                &paths.install_log,
            )?;
        }

        if !paths.model_config.exists() {
            emit_model_event(
                events,
                job_id,
                "download",
                format!("Downloading {MINICPM5_REPO} ({MINICPM5_SIZE})"),
                None,
            );
            fs::create_dir_all(&paths.model_dir).with_context(|| {
                format!("failed to create model dir {}", paths.model_dir.display())
            })?;
            run_logged(
                "download",
                path_str(&paths.python)?,
                &[
                    "-c",
                    "import os; from huggingface_hub import snapshot_download; snapshot_download(os.environ['MINICPM5_REPO'], local_dir=os.environ['MINICPM5_MODEL_DIR'])",
                ],
                &[
                    ("MINICPM5_REPO", MINICPM5_REPO.to_string()),
                    (
                        "MINICPM5_MODEL_DIR",
                        paths.model_dir.display().to_string(),
                    ),
                ],
                &paths.install_log,
            )?;
        } else {
            emit_model_event(
                events,
                job_id,
                "download",
                "Model weights already present".to_string(),
                None,
            );
        }

        emit_model_event(
            events,
            job_id,
            "configure",
            "Installing shim and registering the Puffer provider".to_string(),
            None,
        );
        fs::create_dir_all(&paths.bin_dir)
            .with_context(|| format!("failed to create {}", paths.bin_dir.display()))?;
        fs::create_dir_all(paths.provider.parent().unwrap_or(&paths.puffer_home))?;
        fs::write(&paths.shim, MINICPM5_SHIM)
            .with_context(|| format!("failed to write {}", paths.shim.display()))?;
        fs::write(&paths.provider, MINICPM5_PROVIDER)
            .with_context(|| format!("failed to write {}", paths.provider.display()))?;

        emit_model_event(
            events,
            job_id,
            "serve",
            format!("Starting local server at {MINICPM5_ENDPOINT}"),
            None,
        );
        start_minicpm5_server(&paths)?;
        wait_for_minicpm5_ready()?;

        emit_model_event(
            events,
            job_id,
            "done",
            "MiniCPM5 is installed, registered, and running".to_string(),
            Some(build_status(false)),
        );
        Ok(())
    }
}

fn ensure_minicpm5(model_id: &str) -> Result<()> {
    let normalized = model_id.trim();
    if normalized.is_empty() || normalized == MINICPM5_ID || normalized == MINICPM5_MODEL_ID {
        return Ok(());
    }
    bail!("unsupported local model `{normalized}`")
}

fn build_status(installing: bool) -> LocalModelStatus {
    let paths = minicpm5_paths();
    let supported = supports_minicpm5();
    let installed = paths.model_config.exists() && paths.python.exists() && paths.shim.exists();
    let configured = paths.provider.exists();
    let running = minicpm5_running();
    let recommended = supported && !installed;
    let reason = if !supported {
        unsupported_reason()
    } else if installing {
        "installing".to_string()
    } else if installed && configured && running {
        "ready".to_string()
    } else if installed && configured {
        "installed; server is stopped".to_string()
    } else if installed {
        "installed; provider registration missing".to_string()
    } else {
        "macOS Apple Silicon, model not yet installed".to_string()
    };

    LocalModelStatus {
        id: MINICPM5_ID.to_string(),
        model_id: MINICPM5_MODEL_ID.to_string(),
        display_name: MINICPM5_DISPLAY_NAME.to_string(),
        supported,
        recommended,
        installed,
        configured,
        running,
        installing,
        reason,
        endpoint: MINICPM5_ENDPOINT.to_string(),
        size: MINICPM5_SIZE.to_string(),
        install_path: paths.model_dir.display().to_string(),
        provider_path: paths.provider.display().to_string(),
        log_path: paths.serve_log.display().to_string(),
    }
}

fn minicpm5_paths() -> MiniCpm5Paths {
    let puffer_home = env::var_os("PUFFER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".puffer"));
    let venv = puffer_home.join("venvs/minicpm5");
    let python = venv.join("bin/python");
    let model_dir = puffer_home.join("models/minicpm5-1b");
    let model_config = model_dir.join("config.json");
    let bin_dir = puffer_home.join("bin");
    let shim = bin_dir.join("minicpm5-shim.py");
    let provider = puffer_home.join("resources/providers/minicpm5.yaml");
    let install_log = puffer_home.join("minicpm5-install.log");
    let serve_log = puffer_home.join("minicpm5-serve.log");
    MiniCpm5Paths {
        puffer_home,
        venv,
        python,
        model_dir,
        model_config,
        bin_dir,
        shim,
        provider,
        install_log,
        serve_log,
    }
}

fn supports_minicpm5() -> bool {
    env::consts::OS == "macos" && matches!(env::consts::ARCH, "aarch64" | "arm64")
}

fn unsupported_reason() -> String {
    if env::consts::OS != "macos" {
        return format!(
            "MiniCPM5 MLX install requires macOS; detected {}",
            env::consts::OS
        );
    }
    format!(
        "MiniCPM5 MLX install requires Apple Silicon; detected {}",
        env::consts::ARCH
    )
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn path_str(path: &PathBuf) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

fn python_imports(python: &PathBuf, module: &str) -> bool {
    Command::new(python)
        .arg("-c")
        .arg(format!("import {module}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn reset_log(path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, "")?;
    Ok(())
}

fn append_log(path: &PathBuf, text: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

fn run_logged(
    phase: &str,
    program: &str,
    args: &[&str],
    envs: &[(&str, String)],
    log_path: &PathBuf,
) -> Result<()> {
    append_log(log_path, &format!("\n$ {program} {}\n", args.join(" ")))?;
    let mut command = Command::new(program);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    append_log(log_path, &String::from_utf8_lossy(&output.stdout))?;
    append_log(log_path, &String::from_utf8_lossy(&output.stderr))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{phase} command failed with status {}. See {}",
        output.status,
        log_path.display()
    )
}

fn start_minicpm5_server(paths: &MiniCpm5Paths) -> Result<()> {
    if minicpm5_running() {
        return Ok(());
    }
    if paths.serve_log.exists() {
        let _ = fs::remove_file(&paths.serve_log);
    }
    if let Some(parent) = paths.serve_log.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.serve_log)?;
    let stderr = stdout.try_clone()?;
    Command::new(&paths.python)
        .arg(&paths.shim)
        .env("MINICPM5_MODEL", &paths.model_dir)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("failed to start {}", paths.shim.display()))?;
    Ok(())
}

fn wait_for_minicpm5_ready() -> Result<()> {
    for _ in 0..90 {
        if minicpm5_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    bail!("MiniCPM5 server did not become ready at {MINICPM5_ENDPOINT}")
}

fn minicpm5_running() -> bool {
    let client = match Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(MINICPM5_MODELS_URL)
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn emit_model_event(
    events: &EventEmitter,
    job_id: &str,
    phase: &str,
    message: String,
    status: Option<LocalModelStatus>,
) {
    events.emit(
        MINICPM5_EVENT,
        json!({
            "modelId": MINICPM5_MODEL_ID,
            "jobId": job_id,
            "phase": phase,
            "message": message,
            "status": status,
        }),
    );
}
