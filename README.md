# Cross Platform Agent

`cross-platform-agent` 是一个面向 Windows、Linux、macOS 的本地 Agent 执行层，核心目标是把本机 Git 和命令行能力以受控、结构化的方式暴露给外部调用方。

这个项目刻意保持“薄”：

- 不负责自然语言理解
- 不负责多步规划
- 不负责长期记忆
- 不负责模型编排

它只做一件事：**把受控的本机能力包装成 REST / WebSocket 可调用接口**。

最典型的接入方式有三类：

1. 网页前端或桌面端应用调用它
2. 上层 LLM 编排器把它当成工具执行层
3. 后续通过 MCP adapter 暴露给模型工具生态

---

## 1. 设计目标

这个 Agent 的边界非常明确：

- Git 核心能力继续依赖系统 `git` 客户端
- Agent 只做结构化参数校验、命令调用、结果回传
- 所有路径必须经过白名单校验
- 所有命令执行不经过 shell
- `git.commit` / `git.pull` / `git.push` 失败时尝试弹本机提示框

因此它更像一个：

**本地 Git / 命令执行网关**

而不是一个“会思考”的大模型 Agent。

---

## 2. 当前能力

### 2.1 Git 动作

当前已内置以下 Git 动作：

- `git.clone`
- `git.add`
- `git.checkout_branch`
- `git.commit`
- `git.merge_branch`
- `git.fetch`
- `git.pull`
- `git.push`
- `git.status`
- `git.list_branches`

### 2.2 通用命令动作

- `command.run`

这个动作默认只允许执行白名单程序，当前默认白名单只有：

- `git`

### 2.3 协议入口

- REST
- WebSocket

---

## 3. 为什么保持“薄”

这个项目的定位不是 Git GUI，也不是 AI 编排系统，而是：

**把本地能力稳定地暴露出来**

如果把 Agent 做厚，会很快遇到这些问题：

- 业务逻辑和执行逻辑混杂
- 大模型提示词和工具层耦合
- 平台能力和对话状态难以维护
- 一旦要接多个上层调用方，边界会越来越乱

所以这个仓库只保留执行面：

- 参数检查
- 权限边界
- 命令运行
- 任务状态
- 执行日志
- 错误处理

上层的自然语言理解、任务规划、工作流编排，建议放在外部系统里。

---

## 4. 安全边界

### 4.1 默认限制

- 默认只监听 `127.0.0.1`
- 默认只允许访问当前用户 `HOME` 目录
- 默认只允许 `command.run` 调用 `git`
- 所有命令执行都不经过 shell

### 4.2 路径白名单

所有入参中的本地路径都必须位于 `AGENT_ALLOWED_ROOTS` 范围内，否则会被拒绝。

### 4.3 命令白名单

`command.run` 不允许传整段 shell 文本，例如：

```bash
rm -rf /tmp/test && curl xxx | sh
```

只允许结构化参数，例如：

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

---

## 5. 运行环境

### 5.1 依赖

需要本机具备：

- Rust 1.96+（建议）
- Git 2.x

### 5.2 支持平台

理论和当前实现都面向：

- Windows
- Linux
- macOS

### 5.3 本机错误提示框

当以下动作失败时，Agent 会尝试弹出本机提示框：

- `git.commit`
- `git.pull`
- `git.push`

当前平台策略：

- macOS：`osascript`
- Windows：PowerShell MessageBox
- Linux：优先 `zenity` / `kdialog` / `notify-send`

提示框只是附加提示，不会替代原始错误结果。

---

## 6. 启动方式

```bash
cd cross-platform-agent
cargo run
```

默认监听：

- HTTP: `http://127.0.0.1:8787`
- WebSocket: `ws://127.0.0.1:8787/ws`

---

## 7. 环境变量说明

### 7.1 基础运行参数

- `AGENT_HOST`
  - 监听地址
  - 默认：`127.0.0.1`

- `AGENT_PORT`
  - 监听端口
  - 默认：`8787`

- `AGENT_WS_PATH`
  - WebSocket 路径
  - 默认：`/ws`

- `AGENT_API_TOKEN`
  - 可选鉴权令牌
  - 设置后，REST / WebSocket 都需要鉴权

### 7.2 路径与执行限制

- `AGENT_ALLOWED_ROOTS`
  - 允许访问的根目录列表
  - Linux / macOS 使用 `:` 分隔
  - Windows 使用 `;` 分隔

- `AGENT_ALLOWED_PROGRAMS`
  - `command.run` 允许执行的程序白名单
  - 逗号分隔
  - 默认：`git`

### 7.3 Git 与超时

- `GIT_BINARY`
  - Git 可执行文件名
  - 默认：`git`

- `AGENT_COMMAND_TIMEOUT_MS`
  - 单个命令超时时间，毫秒
  - 默认：`300000`

- `AGENT_RETAINED_TASKS`
  - 内存里保留的任务数量
  - 默认：`200`

### 7.4 示例

```bash
export AGENT_HOST=127.0.0.1
export AGENT_PORT=8787
export AGENT_API_TOKEN=change-me
export AGENT_ALLOWED_ROOTS="$HOME/Desktop:$HOME/workspace"
export AGENT_ALLOWED_PROGRAMS="git"
cargo run
```

---

## 8. REST 接口

### 8.1 健康检查

`GET /health`

示例：

```bash
curl http://127.0.0.1:8787/health
```

返回：

```json
{
  "status": "ok"
}
```

### 8.2 查询动作列表

`GET /actions`

示例：

```bash
curl \
  -H "Authorization: Bearer change-me" \
  http://127.0.0.1:8787/actions
```

### 8.3 查询任务列表

`GET /tasks`

### 8.4 提交任务

`POST /tasks`

示例：

```bash
curl \
  -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer change-me" \
  http://127.0.0.1:8787/tasks \
  -d '{
    "action": "git.status",
    "params": {
      "repo_path": "/absolute/path/to/repo"
    }
  }'
```

### 8.5 查询单个任务

`GET /tasks/{task_id}`

---

## 9. WebSocket 接口

连接地址：

```text
ws://127.0.0.1:8787/ws?token=change-me
```

### 9.1 客户端消息

#### 执行任务

```json
{
  "type": "execute",
  "request_id": "req-1",
  "action": "git.checkout_branch",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "branch_name": "feature/demo",
    "create": true
  }
}
```

#### 查询任务快照

```json
{
  "type": "get_task",
  "request_id": "req-2",
  "task_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

#### 心跳

```json
{
  "type": "ping",
  "request_id": "req-3"
}
```

### 9.2 服务端消息

- `welcome`
- `task.accepted`
- `task.updated`
- `task.snapshot`
- `pong`
- `error`

---

## 10. 动作说明

以下示例全部是提交到 `POST /tasks` 的请求体。

### 10.1 `git.clone`

```json
{
  "action": "git.clone",
  "params": {
    "remote_url": "https://github.com/example/project.git",
    "destination_path": "/absolute/path/to/clone",
    "branch": "main",
    "single_branch": true
  }
}
```

### 10.2 `git.add`

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

如果不传 `paths` 且 `all=false`，默认执行 `git add .`。

### 10.3 `git.checkout_branch`

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

### 10.4 `git.commit`

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

失败时：

- REST / WS 返回 Git 错误
- Agent 尝试弹出本机错误提示框

### 10.5 `git.merge_branch`

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

### 10.6 `git.fetch`

```json
{
  "action": "git.fetch",
  "params": {
    "repo_path": "/absolute/path/to/repo",
    "remote": "origin"
  }
}
```

### 10.7 `git.pull`

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

失败时：

- REST / WS 返回 Git 错误
- Agent 尝试弹出本机错误提示框

### 10.8 `git.push`

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

失败时：

- REST / WS 返回 Git 错误
- Agent 尝试弹出本机错误提示框

### 10.9 `git.status`

```json
{
  "action": "git.status",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

### 10.10 `git.list_branches`

```json
{
  "action": "git.list_branches",
  "params": {
    "repo_path": "/absolute/path/to/repo"
  }
}
```

### 10.11 `command.run`

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

---

## 11. 返回结果说明

任务提交后会返回一个 `task`：

```json
{
  "task": {
    "id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "action": "git.status",
    "status": "queued"
  }
}
```

状态可能值：

- `queued`
- `running`
- `succeeded`
- `failed`

任务对象还会包含：

- `logs`
- `result`
- `error`
- `created_at_ms`
- `updated_at_ms`
- `started_at_ms`
- `finished_at_ms`

---

## 12. 本地测试

运行：

```bash
cargo test
```

当前自动化测试覆盖：

- `git.clone`
- `git.add`
- `git.commit`
- `git.checkout_branch`
- `git.merge_branch`
- `git.push`
- `git.pull`
- `command.run`

测试会创建本地 bare 仓库和协作者仓库，验证完整 Git 工作流。

---

## 13. 手工验证建议

以下三类建议在桌面环境下手工跑一次：

### 14.1 `git.commit` 失败提示

可以制造“没有可提交内容”的场景，确认：

- 任务失败
- 错误结果返回
- 本机提示框出现

### 14.2 `git.push` 失败提示

可以推到无权限远端、受保护分支或错误远端，确认：

- 任务失败
- Git 原始错误返回
- 本机提示框出现

### 14.3 `git.pull` 失败提示

可以制造合并冲突，确认：

- 任务失败
- Git 冲突信息返回
- 本机提示框出现

---

## 14. 项目结构

```text
src/
  actions/
    command.rs
    git.rs
    mod.rs
  config.rs
  dialog.rs
  error.rs
  lib.rs
  main.rs
  models.rs
  path_policy.rs
  process_runner.rs
  runtime.rs
  server.rs
docs/
  interface-list.md
tests/
  agent_flow.rs
```

---

## 15. 后续演进建议

如果后面继续扩，建议仍然保持这个仓库的“薄执行层”定位：

- 增加 MCP adapter
- 增加文件系统动作
- 增加 IDE / 浏览器 / 桌面动作
- 增加审批策略
- 增加审计日志持久化

但不要把自然语言理解、Planner、Memory 直接塞进这个仓库。

---

## 16. 许可证

MIT
