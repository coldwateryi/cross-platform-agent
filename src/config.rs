use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::path_policy::absolutize_path;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub ws_path: String,
    pub api_token: Option<String>,
    pub allowed_roots: Vec<PathBuf>,
    pub command_timeout: Duration,
    pub retained_task_limit: usize,
    pub git_binary: String,
    pub command_allowed_programs: HashSet<String>,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let host = env::var("AGENT_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = parse_u16("AGENT_PORT", 8787)?;
        let ws_path =
            normalize_ws_path(&env::var("AGENT_WS_PATH").unwrap_or_else(|_| "/ws".to_string()))?;
        let api_token = env::var("AGENT_API_TOKEN")
            .ok()
            .filter(|token| !token.trim().is_empty());
        let allowed_roots = parse_allowed_roots(env::var_os("AGENT_ALLOWED_ROOTS"))?;
        let command_timeout =
            Duration::from_millis(parse_u64("AGENT_COMMAND_TIMEOUT_MS", 300_000)?);
        let retained_task_limit = parse_usize("AGENT_RETAINED_TASKS", 200)?;
        let git_binary = env::var("GIT_BINARY").unwrap_or_else(|_| "git".to_string());
        let command_allowed_programs = parse_allowed_programs(
            env::var("AGENT_ALLOWED_PROGRAMS").ok(),
            vec!["git".to_string()],
        );

        Ok(Self {
            host,
            port,
            ws_path,
            api_token,
            allowed_roots,
            command_timeout,
            retained_task_limit,
            git_binary,
            command_allowed_programs,
        })
    }
}

fn normalize_ws_path(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("AGENT_WS_PATH must not be empty."));
    }

    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };

    if prefixed.contains('?') || prefixed.contains('#') {
        return Err(AppError::validation(
            "AGENT_WS_PATH must be a path only, without query string or fragment.",
        ));
    }

    if prefixed.len() > 1 && prefixed.ends_with('/') {
        return Ok(prefixed.trim_end_matches('/').to_string());
    }

    Ok(prefixed)
}

fn parse_u16(key: &str, default: u16) -> AppResult<u16> {
    match env::var(key) {
        Ok(value) => value.parse::<u16>().map_err(|_| {
            AppError::validation(format!("{key} must be a valid unsigned 16-bit integer."))
        }),
        Err(_) => Ok(default),
    }
}

fn parse_u64(key: &str, default: u64) -> AppResult<u64> {
    match env::var(key) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| AppError::validation(format!("{key} must be a valid integer."))),
        Err(_) => Ok(default),
    }
}

fn parse_usize(key: &str, default: usize) -> AppResult<usize> {
    match env::var(key) {
        Ok(value) => value
            .parse::<usize>()
            .map_err(|_| AppError::validation(format!("{key} must be a valid integer."))),
        Err(_) => Ok(default),
    }
}

fn parse_allowed_roots(value: Option<std::ffi::OsString>) -> AppResult<Vec<PathBuf>> {
    let roots = match value {
        Some(raw) if !raw.is_empty() => env::split_paths(&raw)
            .map(|path| absolutize_path(&path.to_string_lossy()))
            .collect::<AppResult<Vec<_>>>()?,
        _ => vec![absolutize_path(
            &env::var("HOME").unwrap_or_else(|_| ".".to_string()),
        )?],
    };

    if roots.is_empty() {
        return Err(AppError::validation(
            "At least one allowed root must be configured.",
        ));
    }

    Ok(roots)
}

fn parse_allowed_programs(value: Option<String>, defaults: Vec<String>) -> HashSet<String> {
    let items = value
        .unwrap_or_else(|| defaults.join(","))
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();

    if items.is_empty() {
        defaults.into_iter().collect()
    } else {
        items
    }
}
