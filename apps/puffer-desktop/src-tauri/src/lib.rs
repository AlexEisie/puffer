mod auth_data;
mod dtos;
mod remote_client;
mod repo_actions;
mod session_data;
mod settings_data;

use crate::dtos::{
    FolderGroupDto, RemoteOperationDto, RepoActionResultDto, RepoStatusDto, SessionDetailDto,
    SettingsSnapshotDto,
};
use anyhow::Result;
use tauri::Builder;

#[tauri::command]
fn list_grouped_sessions(
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<Vec<FolderGroupDto>, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("session-groups")],
        )
        .map_err(|error| error.to_string());
    }
    session_data::list_grouped_sessions().map_err(|error| error.to_string())
}

#[tauri::command]
fn load_session_detail(
    session_id: String,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<SessionDetailDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("session-detail"), session_id],
        )
        .map_err(|error| error.to_string());
    }
    session_data::load_session_detail(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn refresh_repo_status(
    session_id: String,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<RepoStatusDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("repo-status"), session_id],
        )
        .map_err(|error| error.to_string());
    }
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::repo_status(&session_id, &cwd))
}

#[tauri::command]
fn create_pull_request(
    session_id: String,
    title: Option<String>,
    body: Option<String>,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<RepoActionResultDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        let mut args = vec![String::from("create-pull-request"), session_id];
        if let Some(title) = title.as_deref() {
            args.push(String::from("--title"));
            args.push(title.to_string());
        }
        if let Some(body) = body.as_deref() {
            args.push(String::from("--body"));
            args.push(body.to_string());
        }
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &args,
        )
        .map_err(|error| error.to_string());
    }
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::create_pull_request(
        &session_id,
        &cwd,
        title,
        body,
    ))
}

#[tauri::command]
fn merge_pull_request(
    session_id: String,
    pull_request_number: Option<u64>,
    merge_method: Option<String>,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<RepoActionResultDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        let mut args = vec![String::from("merge-pull-request"), session_id];
        if let Some(number) = pull_request_number {
            args.push(String::from("--pull-request-number"));
            args.push(number.to_string());
        }
        if let Some(method) = merge_method.as_deref() {
            args.push(String::from("--merge-method"));
            args.push(method.to_string());
        }
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &args,
        )
        .map_err(|error| error.to_string());
    }
    let cwd = session_data::load_session_cwd(&session_id).map_err(|error| error.to_string())?;
    Ok(repo_actions::merge_pull_request(
        &session_id,
        &cwd,
        pull_request_number,
        merge_method,
    ))
}

#[tauri::command]
fn load_settings_snapshot(
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<SettingsSnapshotDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("settings-snapshot")],
        )
        .map_err(|error| error.to_string());
    }
    settings_data::load_settings_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn login_with_oauth(
    provider_id: String,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
    remote_target: Option<String>,
) -> Result<SettingsSnapshotDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        let snapshot: SettingsSnapshotDto = remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("settings-snapshot")],
        )
        .map_err(|error| error.to_string())?;
        let provider = snapshot
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| format!("unknown remote provider `{provider_id}`"))?;
        if !provider.auth_modes.iter().any(|mode| mode == "oauth") {
            return Err(format!(
                "remote provider `{provider_id}` does not support OAuth"
            ));
        }
        let credential = auth_data::acquire_oauth_credential(&provider_id, &provider.default_api)
            .map_err(|error| error.to_string())?;
        let credential_json =
            serde_json::to_string(&credential).map_err(|error| error.to_string())?;
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[
                String::from("store-credential"),
                provider_id,
                String::from("--credential-json"),
                credential_json,
            ],
        )
        .map_err(|error| error.to_string());
    }
    auth_data::login_with_oauth(&provider_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn login_with_api_key(
    provider_id: String,
    api_key: String,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<SettingsSnapshotDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[
                String::from("login-api-key"),
                provider_id,
                String::from("--api-key"),
                api_key,
            ],
        )
        .map_err(|error| error.to_string());
    }
    auth_data::login_with_api_key(&provider_id, &api_key).map_err(|error| error.to_string())
}

#[tauri::command]
fn logout_provider(
    provider_id: String,
    remote_target: Option<String>,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
) -> Result<SettingsSnapshotDto, String> {
    if let Some(target) = remote_target.filter(|value| !value.trim().is_empty()) {
        return remote_client::run_remote_json(
            &target,
            remote_cwd.as_deref(),
            remote_password.as_deref(),
            &[String::from("logout"), provider_id],
        )
        .map_err(|error| error.to_string());
    }
    auth_data::logout_provider(&provider_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn run_remote_bash(
    remote_target: String,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
    command: String,
) -> Result<RemoteOperationDto, String> {
    remote_client::run_remote_shell(
        &remote_target,
        remote_cwd.as_deref(),
        remote_password.as_deref(),
        &command,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn read_remote_file(
    remote_target: String,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
    path: String,
) -> Result<RemoteOperationDto, String> {
    let command = format!("cat {}", remote_client::shell_quote(&path));
    remote_client::run_remote_shell(
        &remote_target,
        remote_cwd.as_deref(),
        remote_password.as_deref(),
        &command,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn write_remote_file(
    remote_target: String,
    remote_cwd: Option<String>,
    remote_password: Option<String>,
    path: String,
    contents_base64: String,
) -> Result<RemoteOperationDto, String> {
    let command = format!(
        "mkdir -p $(dirname {path}) && printf %s {contents} | base64 -d > {path}",
        path = remote_client::shell_quote(&path),
        contents = remote_client::shell_quote(&contents_base64)
    );
    remote_client::run_remote_shell(
        &remote_target,
        remote_cwd.as_deref(),
        remote_password.as_deref(),
        &command,
    )
    .map_err(|error| error.to_string())
}

/// Runs the Puffer Desktop Tauri host.
pub fn run() {
    Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_grouped_sessions,
            load_session_detail,
            refresh_repo_status,
            create_pull_request,
            merge_pull_request,
            load_settings_snapshot,
            login_with_oauth,
            login_with_api_key,
            logout_provider,
            run_remote_bash,
            read_remote_file,
            write_remote_file
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Puffer Desktop");
}
