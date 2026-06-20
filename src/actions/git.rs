use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::fs;

use crate::dialog::{build_git_error_dialog, show_error_dialog};
use crate::error::{AppError, AppResult};
use crate::runtime::{ActionContext, RunOptions};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CloneParams {
    #[serde(alias = "remoteUrl")]
    remote_url: String,
    #[serde(alias = "destinationPath")]
    destination_path: String,
    branch: Option<String>,
    depth: Option<u32>,
    #[serde(default, alias = "singleBranch")]
    single_branch: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckoutBranchParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    #[serde(alias = "branchName")]
    branch_name: String,
    #[serde(default)]
    create: bool,
    #[serde(alias = "startPoint")]
    start_point: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    paths: Option<Vec<String>>,
    #[serde(default)]
    all: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    message: String,
    #[serde(default)]
    all: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MergeBranchParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    #[serde(alias = "sourceBranch")]
    source_branch: String,
    #[serde(alias = "targetBranch")]
    target_branch: String,
    #[serde(default = "default_true", alias = "noFastForward")]
    no_fast_forward: bool,
    #[serde(alias = "commitMessage")]
    commit_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    remote: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PullParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    remote: Option<String>,
    branch: Option<String>,
    #[serde(default)]
    rebase: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PushParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
    remote: Option<String>,
    branch: Option<String>,
    #[serde(default, alias = "setUpstream")]
    set_upstream: bool,
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPathParams {
    #[serde(alias = "repoPath")]
    repo_path: String,
}

#[derive(Debug, Serialize)]
struct BranchRecord {
    name: String,
    scope: &'static str,
    commit: String,
    current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream: Option<String>,
}

pub async fn clone_repository(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<CloneParams>(params)?;
    let destination_path =
        context.ensure_allowed_path(&params.destination_path, "destination_path")?;

    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent).await.map_err(|error| {
            AppError::internal_with_details(
                "Failed to create parent directory for clone target.",
                json!({
                    "path": parent,
                    "cause": error.to_string(),
                }),
            )
        })?;
    }

    let mut args = vec!["clone".to_string()];
    if let Some(branch) = params.branch {
        args.push("--branch".to_string());
        args.push(branch);
    }
    if let Some(depth) = params.depth {
        args.push("--depth".to_string());
        args.push(depth.to_string());
    }
    if params.single_branch {
        args.push("--single-branch".to_string());
    }
    args.push(params.remote_url.clone());
    args.push(destination_path.to_string_lossy().to_string());

    context
        .run_program(
            &context.config().git_binary,
            &args,
            None,
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "git.clone",
        "repository": describe_repository(&context, destination_path).await?,
    }))
}

pub async fn checkout_branch(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<CheckoutBranchParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let mut args = if params.create {
        vec![
            "checkout".to_string(),
            "-b".to_string(),
            params.branch_name.clone(),
        ]
    } else {
        vec!["checkout".to_string(), params.branch_name.clone()]
    };

    if params.create {
        if let Some(start_point) = params.start_point {
            args.push(start_point);
        }
    }

    context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "git.checkout_branch",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn add(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<AddParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let mut args = vec!["add".to_string()];
    if params.all {
        args.push("--all".to_string());
    }

    match params.paths {
        Some(paths) if !paths.is_empty() => {
            args.extend(paths);
        }
        _ => {
            if !params.all {
                args.push(".".to_string());
            }
        }
    }

    context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "git.add",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn commit(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<CommitParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let mut args = vec!["commit".to_string()];
    if params.all {
        args.push("--all".to_string());
    }
    args.push("-m".to_string());
    args.push(params.message.clone());

    let result = context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await;

    let _ = show_dialog_on_failure("commit", &repo_path, &result).await;
    result?;

    Ok(json!({
        "action": "git.commit",
        "message": params.message,
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn merge_branch(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<MergeBranchParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    context
        .run_program(
            &context.config().git_binary,
            &["checkout".to_string(), params.target_branch.clone()],
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    let mut merge_args = vec!["merge".to_string()];
    if params.no_fast_forward {
        merge_args.push("--no-ff".to_string());
    }
    if let Some(message) = params.commit_message {
        merge_args.push("-m".to_string());
        merge_args.push(message);
    }
    merge_args.push(params.source_branch.clone());

    context
        .run_program(
            &context.config().git_binary,
            &merge_args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "git.merge_branch",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn fetch(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<FetchParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;
    let args = vec![
        "fetch".to_string(),
        params.remote.unwrap_or_else(|| "origin".to_string()),
    ];

    context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await?;

    Ok(json!({
        "action": "git.fetch",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn pull(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<PullParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let mut args = vec!["pull".to_string()];
    if params.rebase {
        args.push("--rebase".to_string());
    }
    if let Some(remote) = params.remote.clone() {
        args.push(remote);
    }
    if let Some(branch) = params.branch.clone() {
        args.push(branch);
    }

    let result = context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await;

    let _ = show_dialog_on_failure("pull", &repo_path, &result).await;
    result?;

    Ok(json!({
        "action": "git.pull",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn push(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<PushParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;
    let remote = params.remote.unwrap_or_else(|| "origin".to_string());
    let branch = match params.branch {
        Some(branch) => branch,
        None => current_branch(&context, repo_path.clone()).await?,
    };

    let mut args = vec!["push".to_string()];
    if params.set_upstream {
        args.push("--set-upstream".to_string());
    }
    if params.force {
        args.push("--force".to_string());
    }
    args.push(remote.clone());
    args.push(branch.clone());

    let result = context
        .run_program(
            &context.config().git_binary,
            &args,
            Some(repo_path.clone()),
            RunOptions {
                enforce_command_allowlist: false,
                log_command: true,
                log_output: true,
            },
        )
        .await;

    let _ = show_dialog_on_failure("push", &repo_path, &result).await;
    result?;

    Ok(json!({
        "action": "git.push",
        "remote": remote,
        "branch": branch,
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn status(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<RepoPathParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    Ok(json!({
        "action": "git.status",
        "repository": describe_repository(&context, repo_path).await?,
    }))
}

pub async fn get_current_branch(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<RepoPathParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;
    let branch = current_branch(&context, repo_path.clone()).await?;

    Ok(json!({
        "action": "git.get_current_branch",
        "repository_path": repo_path,
        "current_branch": branch,
    }))
}

pub async fn diff_staged(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<RepoPathParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let diff = context
        .run_program(
            &context.config().git_binary,
            &["diff".to_string(), "--cached".to_string()],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;
    let files = context
        .run_program(
            &context.config().git_binary,
            &[
                "diff".to_string(),
                "--cached".to_string(),
                "--name-only".to_string(),
            ],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;

    let files = files
        .stdout
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let diff_text = diff.stdout;

    Ok(json!({
        "action": "git.diff_staged",
        "repository_path": repo_path,
        "has_changes": !files.is_empty(),
        "files": files,
        "diff_text": diff_text,
    }))
}

pub async fn list_branches(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<RepoPathParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;

    let output = context
        .run_program(
            &context.config().git_binary,
            &[
                "branch".to_string(),
                "--list".to_string(),
                "--all".to_string(),
                "--verbose".to_string(),
            ],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;

    Ok(json!({
        "action": "git.list_branches",
        "repository_path": repo_path,
        "branches_text": output.stdout.trim(),
    }))
}

pub async fn list_branches_structured(params: Value, context: ActionContext) -> AppResult<Value> {
    let params = parse::<RepoPathParams>(params)?;
    let repo_path = context.ensure_allowed_path(&params.repo_path, "repo_path")?;
    let current = current_branch(&context, repo_path.clone()).await?;

    let local_output = context
        .run_program(
            &context.config().git_binary,
            &[
                "branch".to_string(),
                "--format=%(refname:short)\t%(objectname)\t%(upstream:short)".to_string(),
            ],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;
    let remote_output = context
        .run_program(
            &context.config().git_binary,
            &[
                "branch".to_string(),
                "--remotes".to_string(),
                "--format=%(refname:short)\t%(objectname)".to_string(),
            ],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;

    let mut branches = parse_local_branches(&local_output.stdout, &current);
    branches.extend(parse_remote_branches(&remote_output.stdout));

    Ok(json!({
        "action": "git.list_branches_structured",
        "repository_path": repo_path,
        "branches": branches,
    }))
}

async fn describe_repository(context: &ActionContext, repo_path: PathBuf) -> AppResult<Value> {
    context
        .run_program(
            &context.config().git_binary,
            &["rev-parse".to_string(), "--is-inside-work-tree".to_string()],
            Some(repo_path.clone()),
            RunOptions::QUIET,
        )
        .await?;

    let branch = try_capture(
        context,
        repo_path.clone(),
        &[
            "rev-parse".to_string(),
            "--abbrev-ref".to_string(),
            "HEAD".to_string(),
        ],
    )
    .await;
    let head = try_capture(
        context,
        repo_path.clone(),
        &["rev-parse".to_string(), "HEAD".to_string()],
    )
    .await;
    let status = try_capture(
        context,
        repo_path.clone(),
        &[
            "status".to_string(),
            "--short".to_string(),
            "--branch".to_string(),
        ],
    )
    .await;

    Ok(json!({
        "repository_path": repo_path,
        "current_branch": branch,
        "head_commit": head,
        "status_text": status,
    }))
}

async fn current_branch(context: &ActionContext, repo_path: PathBuf) -> AppResult<String> {
    let output = context
        .run_program(
            &context.config().git_binary,
            &[
                "rev-parse".to_string(),
                "--abbrev-ref".to_string(),
                "HEAD".to_string(),
            ],
            Some(repo_path),
            RunOptions::QUIET,
        )
        .await?;

    let branch = output.stdout.trim().to_string();
    if branch.is_empty() {
        return Err(AppError::validation("Failed to resolve current branch."));
    }

    Ok(branch)
}

async fn show_dialog_on_failure(
    action: &str,
    repo_path: &PathBuf,
    result: &AppResult<crate::process_runner::ProcessOutput>,
) -> AppResult<()> {
    if let Err(error) = result {
        show_error_dialog(build_git_error_dialog(
            action,
            &repo_path.to_string_lossy(),
            error,
        ))
        .await?;
    }

    Ok(())
}

async fn try_capture(
    context: &ActionContext,
    repo_path: PathBuf,
    args: &[String],
) -> Option<String> {
    context
        .run_program(
            &context.config().git_binary,
            args,
            Some(repo_path),
            RunOptions::QUIET,
        )
        .await
        .ok()
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_local_branches(output: &str, current_branch: &str) -> Vec<BranchRecord> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            let commit = fields.next()?.trim();
            let upstream = fields
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);

            if name.is_empty() || commit.is_empty() {
                return None;
            }

            Some(BranchRecord {
                name: name.to_string(),
                scope: "local",
                commit: commit.to_string(),
                current: name == current_branch,
                upstream,
            })
        })
        .collect()
}

fn parse_remote_branches(output: &str) -> Vec<BranchRecord> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            let commit = fields.next()?.trim();

            if name.is_empty() || commit.is_empty() {
                return None;
            }

            Some(BranchRecord {
                name: name.to_string(),
                scope: "remote",
                commit: commit.to_string(),
                current: false,
                upstream: None,
            })
        })
        .collect()
}

fn parse<T>(value: Value) -> AppResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value)
        .map_err(|error| AppError::validation(format!("Invalid action parameters: {error}")))
}

const fn default_true() -> bool {
    true
}
