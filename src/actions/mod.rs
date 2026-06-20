mod command;
mod git;

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{ActionDescriptor, ActionParamDescriptor};
use crate::runtime::ActionContext;

pub fn supports(action: &str) -> bool {
    matches!(
        action,
        "git.clone"
            | "git.add"
            | "git.checkout_branch"
            | "git.commit"
            | "git.get_current_branch"
            | "git.diff_staged"
            | "git.merge_branch"
            | "git.pull"
            | "git.push"
            | "git.fetch"
            | "git.status"
            | "git.list_branches"
            | "git.list_branches_structured"
            | "command.run"
    )
}

pub fn catalog() -> Vec<ActionDescriptor> {
    vec![
        ActionDescriptor {
            name: "git.clone",
            description: "Clone a Git repository into an allowed local directory.",
            params: vec![
                ActionParamDescriptor {
                    name: "remote_url",
                    kind: "string",
                    required: true,
                    description: "Remote repository URL.",
                },
                ActionParamDescriptor {
                    name: "destination_path",
                    kind: "string",
                    required: true,
                    description: "Local clone destination path.",
                },
                ActionParamDescriptor {
                    name: "branch",
                    kind: "string",
                    required: false,
                    description: "Optional branch to clone.",
                },
                ActionParamDescriptor {
                    name: "depth",
                    kind: "integer",
                    required: false,
                    description: "Optional shallow clone depth.",
                },
                ActionParamDescriptor {
                    name: "single_branch",
                    kind: "boolean",
                    required: false,
                    description: "Whether to clone only the requested branch.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.add",
            description: "Stage files in a local repository.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "paths",
                    kind: "array<string>",
                    required: false,
                    description: "Relative paths to stage. If omitted, stages all changes.",
                },
                ActionParamDescriptor {
                    name: "all",
                    kind: "boolean",
                    required: false,
                    description: "Stage all tracked and untracked changes.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.checkout_branch",
            description: "Checkout an existing branch or create a new one.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "branch_name",
                    kind: "string",
                    required: true,
                    description: "Target branch name.",
                },
                ActionParamDescriptor {
                    name: "create",
                    kind: "boolean",
                    required: false,
                    description: "Create the branch if it does not already exist.",
                },
                ActionParamDescriptor {
                    name: "start_point",
                    kind: "string",
                    required: false,
                    description: "Optional branch or commit used when creating the branch.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.commit",
            description: "Create a commit in a local repository.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "message",
                    kind: "string",
                    required: true,
                    description: "Commit message.",
                },
                ActionParamDescriptor {
                    name: "all",
                    kind: "boolean",
                    required: false,
                    description: "Commit all tracked changes without a separate add step.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.merge_branch",
            description: "Checkout the target branch and merge another branch into it.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "source_branch",
                    kind: "string",
                    required: true,
                    description: "Branch to merge from.",
                },
                ActionParamDescriptor {
                    name: "target_branch",
                    kind: "string",
                    required: true,
                    description: "Branch to merge into.",
                },
                ActionParamDescriptor {
                    name: "no_fast_forward",
                    kind: "boolean",
                    required: false,
                    description: "Create a merge commit instead of fast-forwarding.",
                },
                ActionParamDescriptor {
                    name: "commit_message",
                    kind: "string",
                    required: false,
                    description: "Optional custom merge commit message.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.pull",
            description: "Pull changes from a remote repository.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "remote",
                    kind: "string",
                    required: false,
                    description: "Remote name, default origin.",
                },
                ActionParamDescriptor {
                    name: "branch",
                    kind: "string",
                    required: false,
                    description: "Branch to pull. If omitted, current upstream is used.",
                },
                ActionParamDescriptor {
                    name: "rebase",
                    kind: "boolean",
                    required: false,
                    description: "Use rebase while pulling.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.push",
            description: "Push a local branch to a remote repository.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "remote",
                    kind: "string",
                    required: false,
                    description: "Remote name, default origin.",
                },
                ActionParamDescriptor {
                    name: "branch",
                    kind: "string",
                    required: false,
                    description: "Branch to push. If omitted, current branch is used.",
                },
                ActionParamDescriptor {
                    name: "set_upstream",
                    kind: "boolean",
                    required: false,
                    description: "Set upstream while pushing.",
                },
                ActionParamDescriptor {
                    name: "force",
                    kind: "boolean",
                    required: false,
                    description: "Force push the branch.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.fetch",
            description: "Fetch updates from a remote repository.",
            params: vec![
                ActionParamDescriptor {
                    name: "repo_path",
                    kind: "string",
                    required: true,
                    description: "Local repository path.",
                },
                ActionParamDescriptor {
                    name: "remote",
                    kind: "string",
                    required: false,
                    description: "Remote name, default origin.",
                },
            ],
        },
        ActionDescriptor {
            name: "git.status",
            description: "Read current branch and repository status.",
            params: vec![ActionParamDescriptor {
                name: "repo_path",
                kind: "string",
                required: true,
                description: "Local repository path.",
            }],
        },
        ActionDescriptor {
            name: "git.get_current_branch",
            description: "Read the current local branch name.",
            params: vec![ActionParamDescriptor {
                name: "repo_path",
                kind: "string",
                required: true,
                description: "Local repository path.",
            }],
        },
        ActionDescriptor {
            name: "git.diff_staged",
            description: "Read the currently staged diff and staged file list.",
            params: vec![ActionParamDescriptor {
                name: "repo_path",
                kind: "string",
                required: true,
                description: "Local repository path.",
            }],
        },
        ActionDescriptor {
            name: "git.list_branches",
            description: "List local and remote branches.",
            params: vec![ActionParamDescriptor {
                name: "repo_path",
                kind: "string",
                required: true,
                description: "Local repository path.",
            }],
        },
        ActionDescriptor {
            name: "git.list_branches_structured",
            description: "List local and remote branches as structured JSON.",
            params: vec![ActionParamDescriptor {
                name: "repo_path",
                kind: "string",
                required: true,
                description: "Local repository path.",
            }],
        },
        ActionDescriptor {
            name: "command.run",
            description: "Run an allowlisted executable with structured arguments and no shell.",
            params: vec![
                ActionParamDescriptor {
                    name: "program",
                    kind: "string",
                    required: true,
                    description: "Executable name from the allowlist.",
                },
                ActionParamDescriptor {
                    name: "args",
                    kind: "array<string>",
                    required: false,
                    description: "Structured argument list.",
                },
                ActionParamDescriptor {
                    name: "cwd",
                    kind: "string",
                    required: false,
                    description: "Optional working directory within allowed roots.",
                },
            ],
        },
    ]
}

pub async fn execute(action: &str, params: Value, context: ActionContext) -> AppResult<Value> {
    match action {
        "git.clone" => git::clone_repository(params, context).await,
        "git.add" => git::add(params, context).await,
        "git.checkout_branch" => git::checkout_branch(params, context).await,
        "git.commit" => git::commit(params, context).await,
        "git.get_current_branch" => git::get_current_branch(params, context).await,
        "git.diff_staged" => git::diff_staged(params, context).await,
        "git.merge_branch" => git::merge_branch(params, context).await,
        "git.pull" => git::pull(params, context).await,
        "git.push" => git::push(params, context).await,
        "git.fetch" => git::fetch(params, context).await,
        "git.status" => git::status(params, context).await,
        "git.list_branches" => git::list_branches(params, context).await,
        "git.list_branches_structured" => git::list_branches_structured(params, context).await,
        "command.run" => command::run(params, context).await,
        _ => Err(AppError::validation(format!(
            "Unsupported action: {action}"
        ))),
    }
}
