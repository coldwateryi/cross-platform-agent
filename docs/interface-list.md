# Cross Platform Agent Interface List

## Overview

- Base URL: `http://127.0.0.1:8787`
- WebSocket URL: `ws://127.0.0.1:8787/ws`
- Auth:
  - `Authorization: Bearer <token>`
  - or `X-API-Token: <token>`
  - or WebSocket query parameter `?token=<token>`
- Content type: `application/json`

`/health` does not require auth. Other REST endpoints and `/ws` require auth only when `AGENT_API_TOKEN` is configured.

## REST Endpoints

### `GET /health`

Purpose:
- Health check

Response example:

```json
{
  "status": "ok"
}
```

### `GET /actions`

Purpose:
- List supported structured actions and parameter definitions

Response example:

```json
{
  "actions": [
    {
      "name": "git.clone",
      "description": "Clone a Git repository into an allowed local directory.",
      "params": [
        {
          "name": "remote_url",
          "kind": "string",
          "required": true,
          "description": "Remote repository URL."
        }
      ]
    }
  ]
}
```

### `GET /tasks`

Purpose:
- List retained task records

Response example:

```json
{
  "tasks": []
}
```

### `POST /tasks`

Purpose:
- Submit a structured action as an async task

Request body:

```json
{
  "action": "git.status",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

Response:
- `202 Accepted`

Response example:

```json
{
  "task": {
    "id": "27a9e3f8-b9fd-49f8-a812-ea8f65df0fe8",
    "action": "git.status",
    "params": {
      "repo_path": "/absolute/path/to/repo"
    },
    "status": "queued",
    "created_at_ms": 1761213544201,
    "updated_at_ms": 1761213544201,
    "logs": []
  }
}
```

### `GET /tasks/{task_id}`

Purpose:
- Query one task snapshot

Response example:

```json
{
  "task": {
    "id": "27a9e3f8-b9fd-49f8-a812-ea8f65df0fe8",
    "action": "git.status",
    "status": "succeeded",
    "result": {
      "action": "git.status",
      "repository": {
        "repository_path": "/absolute/path/to/repo",
        "current_branch": "main",
        "head_commit": "abc123",
        "status_text": "## main"
      }
    }
  }
}
```

## Supported Actions

### `git.clone`

```json
{
  "action": "git.clone",
  "params": {
    "remote_url": "git@github.com:org/repo.git",
    "destination_path": "/absolute/path/to/clone",
    "branch": "main",
    "depth": 1,
    "single_branch": true
  }
}
```

### `git.add`

```json
{
  "action": "git.add",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "paths": ["README.md"],
    "all": false
  }
}
```

### `git.checkout_branch`

```json
{
  "action": "git.checkout_branch",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "branch_name": "feature/demo",
    "create": true,
    "start_point": "main"
  }
}
```

### `git.commit`

If commit fails, the agent also attempts to show a native error dialog on the local machine.

```json
{
  "action": "git.commit",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "message": "Update README",
    "all": false
  }
}
```

### `git.merge_branch`

```json
{
  "action": "git.merge_branch",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "source_branch": "feature/demo",
    "target_branch": "main",
    "no_fast_forward": true,
    "commit_message": "Merge feature/demo into main"
  }
}
```

### `git.fetch`

```json
{
  "action": "git.fetch",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "remote": "origin"
  }
}
```

### `git.pull`

If pull fails, the agent also attempts to show a native error dialog on the local machine.

```json
{
  "action": "git.pull",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "remote": "origin",
    "branch": "main",
    "rebase": false
  }
}
```

### `git.push`

If push fails, the agent also attempts to show a native error dialog on the local machine.

```json
{
  "action": "git.push",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "remote": "origin",
    "branch": "main",
    "set_upstream": false,
    "force": false
  }
}
```

### `git.status`

```json
{
  "action": "git.status",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

### `git.list_branches`

```json
{
  "action": "git.list_branches",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

### `command.run`

This action is intentionally restricted:

- no shell string
- no pipeline
- no redirection
- program must be in allowlist

```json
{
  "action": "command.run",
  "params": {
    "program": "git",
    "args": ["status", "--short"],
    "cwd": "/absolute/path/to/repo"
  }
}
```

## WebSocket

### Connect

`ws://127.0.0.1:8787/ws?token=<token>`

### Client messages

Execute task:

```json
{
  "type": "execute",
  "request_id": "req-1",
  "action": "git.status",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

Query task snapshot:

```json
{
  "type": "get_task",
  "request_id": "req-2",
  "task_id": "27a9e3f8-b9fd-49f8-a812-ea8f65df0fe8"
}
```

Heartbeat:

```json
{
  "type": "ping",
  "request_id": "req-3"
}
```

### Server messages

- `welcome`
- `task.accepted`
- `task.updated`
- `task.snapshot`
- `pong`
- `error`

## Notes

- All file paths must remain within `AGENT_ALLOWED_ROOTS`.
- `git.merge_branch` may require `git config user.name` and `git config user.email` in the target repo if a merge commit is created.
- `git.commit`, `git.pull`, and `git.push` try to raise a local native error dialog when they fail.
