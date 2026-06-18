use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{AppError, AppResult};
use crate::runtime::{ActionContext, RunOptions};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandRunParams {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<String>,
}

pub async fn run(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = serde_json::from_value::<CommandRunParams>(params)
        .map_err(|error| AppError::validation(format!("Invalid action parameters: {error}")))?;

    if params.program.trim().is_empty() {
        return Err(AppError::validation("program must not be empty."));
    }

    let cwd = match params.cwd {
        Some(path) => Some(context.ensure_allowed_path(&path, "cwd")?),
        None => None,
    };

    let output = context
        .run_program(
            &params.program,
            &params.args,
            cwd.clone(),
            RunOptions {
                enforce_command_allowlist: true,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "command.run",
        "program": params.program,
        "args": params.args,
        "cwd": cwd,
        "exit_code": output.exit_code,
        "stdout": output.stdout,
        "stderr": output.stderr,
    }))
}
