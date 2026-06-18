use std::process::Stdio;

use tokio::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct DialogRequest {
    pub title: String,
    pub message: String,
}

pub async fn show_error_dialog(request: DialogRequest) -> AppResult<()> {
    let commands = platform_dialog_commands(&request);
    if commands.is_empty() {
        return Ok(());
    }

    for (program, args) in commands {
        let status = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if let Ok(status) = status {
            if status.success() {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn platform_dialog_commands(request: &DialogRequest) -> Vec<(&'static str, Vec<String>)> {
    if cfg!(target_os = "macos") {
        return vec![(
            "osascript",
            vec![
                "-e".to_string(),
                format!(
                    "display dialog {} with title {} buttons {{\"OK\"}} default button \"OK\" with icon caution",
                    apple_quote(&request.message),
                    apple_quote(&request.title)
                ),
            ],
        )];
    }

    if cfg!(target_os = "windows") {
        return vec![(
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                format!(
                    "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show({}, {}, 'OK', 'Error') | Out-Null",
                    ps_quote(&request.message),
                    ps_quote(&request.title)
                ),
            ],
        )];
    }

    vec![
        (
            "zenity",
            vec![
                "--error".to_string(),
                "--title".to_string(),
                request.title.clone(),
                "--text".to_string(),
                request.message.clone(),
            ],
        ),
        (
            "kdialog",
            vec![
                "--error".to_string(),
                request.message.clone(),
                "--title".to_string(),
                request.title.clone(),
            ],
        ),
        (
            "notify-send",
            vec![
                request.title.clone(),
                request.message.clone(),
                "--urgency=critical".to_string(),
            ],
        ),
    ]
}

fn apple_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn build_git_error_dialog(action: &str, repo_path: &str, error: &AppError) -> DialogRequest {
    DialogRequest {
        title: format!("Git {} failed", action),
        message: format!(
            "Repository: {}\n\n{}\n\nCode: {}",
            repo_path,
            error,
            error.payload().code
        ),
    }
}
