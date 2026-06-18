use std::path::{Component, Path, PathBuf};

use serde_json::json;

use crate::error::{AppError, AppResult};

pub fn absolutize_path(input: &str) -> AppResult<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("Path must not be empty."));
    }

    let raw = PathBuf::from(trimmed);
    let joined = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()
            .map_err(|error| {
                AppError::internal_with_details(
                    "Failed to read current working directory.",
                    json!({ "cause": error.to_string() }),
                )
            })?
            .join(raw)
    };

    Ok(normalize_path(&joined))
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }

    normalized
}

fn comparable_path(path: &Path) -> String {
    let normalized = normalize_path(path).to_string_lossy().to_string();
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

pub fn ensure_allowed_path(input: &str, allowed_roots: &[PathBuf], field_name: &str) -> AppResult<PathBuf> {
    let resolved = absolutize_path(input)?;
    let candidate = comparable_path(&resolved);

    let allowed = allowed_roots.iter().any(|root| {
        let root_cmp = comparable_path(root);
        candidate == root_cmp || candidate.starts_with(&format!("{root_cmp}{}", std::path::MAIN_SEPARATOR))
    });

    if allowed {
        return Ok(resolved);
    }

    Err(AppError::validation_with_details(
        format!("{field_name} must stay within an allowed root."),
        json!({
            "field": field_name,
            "resolved_path": resolved,
            "allowed_roots": allowed_roots,
        }),
    ))
}
