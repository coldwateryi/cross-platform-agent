use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_process(
    program: &str,
    args: &[String],
    cwd: Option<PathBuf>,
    timeout: Duration,
) -> AppResult<ProcessOutput> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(ref directory) = cwd {
        command.current_dir(directory);
    }

    let mut child = command.spawn().map_err(|error| {
        AppError::process_failed(
            format!("Failed to start program: {program}"),
            json!({
                "program": program,
                "args": args,
                "cwd": cwd,
                "cause": error.to_string(),
            }),
        )
    })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
        match stdout {
            Some(mut reader) => {
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).await?;
                Ok::<String, std::io::Error>(String::from_utf8_lossy(&bytes).into_owned())
            }
            None => Ok(String::new()),
        }
    });

    let stderr_task = tokio::spawn(async move {
        match stderr {
            Some(mut reader) => {
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).await?;
                Ok::<String, std::io::Error>(String::from_utf8_lossy(&bytes).into_owned())
            }
            None => Ok(String::new()),
        }
    });

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map_err(|error| {
            AppError::process_failed(
                format!("Failed while waiting for program: {program}"),
                json!({
                    "program": program,
                    "args": args,
                    "cwd": cwd,
                    "cause": error.to_string(),
                }),
            )
        })?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;

            let stdout = join_output(stdout_task).await?;
            let stderr = join_output(stderr_task).await?;

            return Err(AppError::process_failed(
                format!("Program timed out: {program}"),
                json!({
                    "program": program,
                    "args": args,
                    "cwd": cwd,
                    "timeout_ms": timeout.as_millis(),
                    "stdout": stdout,
                    "stderr": stderr,
                }),
            ));
        }
    };

    let stdout = join_output(stdout_task).await?;
    let stderr = join_output(stderr_task).await?;
    let exit_code = status.code().unwrap_or(-1);

    if status.success() {
        return Ok(ProcessOutput {
            exit_code,
            stdout,
            stderr,
        });
    }

    Err(AppError::process_failed(
        format!("Program exited with a non-zero status: {program}"),
        json!({
            "program": program,
            "args": args,
            "cwd": cwd,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
        }),
    ))
}

async fn join_output(
    handle: tokio::task::JoinHandle<Result<String, std::io::Error>>,
) -> AppResult<String> {
    handle
        .await
        .map_err(|error| {
            AppError::internal_with_details(
                "Failed to join process output task.",
                json!({ "cause": error.to_string() }),
            )
        })?
        .map_err(|error| {
            AppError::internal_with_details(
                "Failed to read process output.",
                json!({ "cause": error.to_string() }),
            )
        })
}
