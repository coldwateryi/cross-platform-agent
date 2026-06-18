use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::actions;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::models::{ExecuteTaskRequest, LogStream, TaskLogEntry, TaskRecord, TaskStatus};
use crate::path_policy::ensure_allowed_path;
use crate::process_runner::{ProcessOutput, run_process};

#[derive(Clone)]
pub struct Runtime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    config: Config,
    tasks: Mutex<TaskStore>,
    updates: broadcast::Sender<TaskRecord>,
}

struct TaskStore {
    tasks: HashMap<Uuid, TaskRecord>,
    order: VecDeque<Uuid>,
}

#[derive(Clone)]
pub struct ActionContext {
    runtime: Runtime,
    task_id: Uuid,
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub enforce_command_allowlist: bool,
    pub log_command: bool,
    pub log_output: bool,
}

impl RunOptions {
    pub const QUIET: Self = Self {
        enforce_command_allowlist: false,
        log_command: false,
        log_output: false,
    };
}

impl Runtime {
    pub fn new(config: Config) -> Self {
        let (updates, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(RuntimeInner {
                config,
                tasks: Mutex::new(TaskStore {
                    tasks: HashMap::new(),
                    order: VecDeque::new(),
                }),
                updates,
            }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskRecord> {
        self.inner.updates.subscribe()
    }

    pub async fn list_tasks(&self) -> Vec<TaskRecord> {
        let store = self.inner.tasks.lock().await;
        let mut items = store.tasks.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));
        items
    }

    pub async fn get_task(&self, task_id: Uuid) -> Option<TaskRecord> {
        let store = self.inner.tasks.lock().await;
        store.tasks.get(&task_id).cloned()
    }

    pub async fn submit_task(&self, request: ExecuteTaskRequest) -> AppResult<TaskRecord> {
        if !actions::supports(&request.action) {
            return Err(AppError::validation(format!(
                "Unsupported action: {}",
                request.action
            )));
        }

        let now = now_ms();
        let task = TaskRecord {
            id: Uuid::new_v4(),
            action: request.action.clone(),
            params: request.params.clone(),
            status: TaskStatus::Queued,
            created_at_ms: now,
            updated_at_ms: now,
            started_at_ms: None,
            finished_at_ms: None,
            logs: Vec::new(),
            result: None,
            error: None,
        };

        {
            let mut store = self.inner.tasks.lock().await;
            store.order.push_back(task.id);
            store.tasks.insert(task.id, task.clone());

            while store.order.len() > self.inner.config.retained_task_limit {
                if let Some(oldest_id) = store.order.pop_front() {
                    store.tasks.remove(&oldest_id);
                }
            }
        }

        self.broadcast(task.clone());

        let runtime = self.clone();
        tokio::spawn(async move {
            runtime.execute_task(task.id).await;
        });

        Ok(task)
    }

    pub async fn append_log(
        &self,
        task_id: Uuid,
        stream: LogStream,
        message: impl Into<String>,
    ) -> AppResult<()> {
        let updated = {
            let mut store = self.inner.tasks.lock().await;
            let task = store
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| AppError::not_found("Task not found."))?;

            task.logs.push(TaskLogEntry {
                timestamp_ms: now_ms(),
                stream,
                message: message.into(),
            });
            if task.logs.len() > 500 {
                let split_at = task.logs.len() - 500;
                task.logs.drain(0..split_at);
            }
            task.updated_at_ms = now_ms();
            task.clone()
        };

        self.broadcast(updated);
        Ok(())
    }

    async fn execute_task(&self, task_id: Uuid) {
        if let Err(error) = self.mark_running(task_id).await {
            let _ = self.fail_task(task_id, error).await;
            return;
        }

        let (action, params) = {
            let store = self.inner.tasks.lock().await;
            let Some(task) = store.tasks.get(&task_id) else {
                return;
            };
            (task.action.clone(), task.params.clone())
        };

        let context = ActionContext {
            runtime: self.clone(),
            task_id,
        };

        match actions::execute(&action, params, context).await {
            Ok(result) => {
                let _ = self.succeed_task(task_id, result).await;
            }
            Err(error) => {
                let _ = self.fail_task(task_id, error).await;
            }
        }
    }

    async fn mark_running(&self, task_id: Uuid) -> AppResult<()> {
        let updated = {
            let mut store = self.inner.tasks.lock().await;
            let task = store
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| AppError::not_found("Task not found."))?;

            let now = now_ms();
            task.status = TaskStatus::Running;
            task.started_at_ms = Some(now);
            task.updated_at_ms = now;
            task.clone()
        };

        self.broadcast(updated);
        Ok(())
    }

    async fn succeed_task(&self, task_id: Uuid, result: Value) -> AppResult<()> {
        let updated = {
            let mut store = self.inner.tasks.lock().await;
            let task = store
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| AppError::not_found("Task not found."))?;

            let now = now_ms();
            task.status = TaskStatus::Succeeded;
            task.result = Some(result);
            task.finished_at_ms = Some(now);
            task.updated_at_ms = now;
            task.clone()
        };

        self.broadcast(updated);
        Ok(())
    }

    async fn fail_task(&self, task_id: Uuid, error: AppError) -> AppResult<()> {
        let updated = {
            let mut store = self.inner.tasks.lock().await;
            let task = store
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| AppError::not_found("Task not found."))?;

            let now = now_ms();
            task.status = TaskStatus::Failed;
            task.error = Some(error.payload());
            task.finished_at_ms = Some(now);
            task.updated_at_ms = now;
            task.clone()
        };

        self.broadcast(updated);
        Ok(())
    }

    fn broadcast(&self, task: TaskRecord) {
        let _ = self.inner.updates.send(task);
    }
}

impl ActionContext {
    pub fn config(&self) -> &Config {
        self.runtime.config()
    }

    pub async fn log(&self, stream: LogStream, message: impl Into<String>) -> AppResult<()> {
        self.runtime.append_log(self.task_id, stream, message).await
    }

    pub fn ensure_allowed_path(&self, input: &str, field_name: &str) -> AppResult<std::path::PathBuf> {
        ensure_allowed_path(input, &self.config().allowed_roots, field_name)
    }

    pub async fn run_program(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<std::path::PathBuf>,
        options: RunOptions,
    ) -> AppResult<ProcessOutput> {
        if options.enforce_command_allowlist {
            ensure_program_allowed(program, self.config())?;
        }

        if options.log_command {
            self.log(
                LogStream::System,
                format!("$ {} {}", program, args.join(" ")),
            )
            .await?;
        }

        let output = run_process(program, args, cwd.clone(), self.config().command_timeout).await?;

        if options.log_output {
            for line in output.stdout.lines() {
                self.log(LogStream::Stdout, line.to_string()).await?;
            }
            for line in output.stderr.lines() {
                self.log(LogStream::Stderr, line.to_string()).await?;
            }
        }

        Ok(output)
    }
}

fn ensure_program_allowed(program: &str, config: &Config) -> AppResult<()> {
    if program.contains('/') || program.contains('\\') {
        return Err(AppError::validation(
            "program must be a bare executable name, not a path.",
        ));
    }

    if config.command_allowed_programs.contains(program) {
        return Ok(());
    }

    Err(AppError::forbidden(format!(
        "Program is not allowed: {program}"
    )))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
