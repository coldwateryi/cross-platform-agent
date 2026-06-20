# Cross Platform Agent 需求适配分析

## 1. 文档目的

本文档用于汇总当前对 `cross-platform-agent` 项目的分析结论，重点说明：

- 当前项目的设计定位与实现边界
- 它是否能够支撑目标项目管理软件驱动的 Git 工作流
- 哪些职责应该保留在本地 agent 中
- 哪些职责应该放在上层系统中实现
- 当前存在的缺口、风险和建议的下一步工作

分析日期：2026-06-20

---

## 2. 当前项目定位

`cross-platform-agent` 是一个面向 Windows、Linux、macOS 的本地薄执行层。

它的设计目标非常明确：

- 通过 REST / WebSocket 暴露本机 Git 和命令行能力
- 对输入参数进行结构化校验
- 对路径和命令执行边界进行约束
- 所有命令执行都不经过 shell
- 返回任务状态、日志、结果和错误

它明确**不负责**以下事项：

- 自然语言理解
- 多步任务规划
- 长期记忆
- 模型编排
- 项目管理领域逻辑

从工程定位上看，这个项目本质上是一个**本地 Git / 命令执行网关**，而不是项目管理后端，也不是 AI 编排 agent。

---

## 3. 当前代码库概览

### 3.1 主要结构

- `src/main.rs`
  - 启动服务
  - 从环境变量读取配置
  - 创建 runtime
  - 绑定 HTTP/WebSocket 监听端口

- `src/server.rs`
  - 定义 REST 和 WebSocket 路由
  - 处理鉴权
  - 接收任务请求
  - 推送任务状态更新

- `src/runtime.rs`
  - 管理异步任务生命周期
  - 保存任务状态
  - 异步执行动作
  - 记录日志和状态迁移

- `src/actions/git.rs`
  - 实现内置 Git 动作

- `src/actions/command.rs`
  - 实现通用 `command.run`
  - 执行程序白名单校验

- `src/path_policy.rs`
  - 校验本地路径必须落在允许的根目录范围内

- `src/process_runner.rs`
  - 无 shell 执行子进程
  - 收集 stdout/stderr
  - 应用超时控制

### 3.2 暴露协议

当前支持的接入方式：

- REST
- WebSocket

主要接口：

- `GET /health`
- `GET /actions`
- `GET /tasks`
- `POST /tasks`
- `GET /tasks/{task_id}`
- `GET /ws`（默认路径，实际路径可由 `AGENT_WS_PATH` 配置）

### 3.3 当前内置动作

当前支持的动作包括：

- `git.clone`
- `git.add`
- `git.checkout_branch`
- `git.commit`
- `git.merge_branch`
- `git.fetch`
- `git.pull`
- `git.push`
- `git.status`
- `git.get_current_branch`
- `git.diff_staged`
- `git.list_branches`
- `git.list_branches_structured`
- `command.run`

### 3.4 当前测试状态

截至 2026-06-20，`cargo test` 已通过。

当前集成测试覆盖的主流程包括：

- clone
- checkout branch
- add
- commit
- merge
- push
- pull
- `command.run`

这说明仓库当前作为本地 Git 执行层，在主路径上是自洽的。

---

## 4. 目标用户工作流

本次分析中讨论的目标业务流程如下：

1. 在项目管理软件中，对需求做 WBS 任务拆分，并把任务指派给开发人员。
2. 开发人员登录后，查看分配给自己的任务。
3. 用户在任务清单中可以执行本地 Git 操作：
   - clone 项目代码
   - pull 项目代码
   - 将选中的分支设为当前分支
   - 从当前分支向目标分支提交 merge request
4. 用户点击“开发当前任务”时：
   - 如有需要先 clone 仓库
   - 切换到指定基础分支
   - 基于当前分支创建新分支
   - 分支名由任务 ID 和任务简述构成
5. 用户点击“commit 当前代码”时：
   - 对任务相关代码执行 add 和 commit
   - commit 前调用 LLM 检查 staged 代码是否符合任务要求
   - 只有满足要求时才执行 commit

---

## 5. 职责边界

在需求进一步澄清后，最终边界如下：

### 5.1 Agent 负责的内容

本地 agent 只负责：

- 在开发机上执行本地 Git 操作
- 结构化命令调用
- 路径策略校验
- 异步任务状态回传
- 执行日志与错误信息输出

### 5.2 上层系统负责的内容

项目管理系统或其后端负责：

- WBS 拆分
- 任务指派
- 用户身份和权限
- 任务详情与任务描述
- 工作区路径配置存储
- 任务分支命名规则
- LLM 编排与审核决策
- 通过 Git 托管平台 API 创建 MR / PR

这个职责边界与当前仓库设计是一致的。

---

## 6. 需求适配评估

## 6.1 需求 1：WBS 拆分与任务指派

需求内容：

- 将需求拆成 WBS 任务
- 指派开发人员
- 用户登录后查看自己的任务

评估结论：

- **当前 agent 不负责**
- **应保留在上层系统实现**

原因：

当前 agent 没有以下领域模型：

- 用户
- 项目
- 需求
- WBS
- 任务指派
- 业务任务

另外需要特别说明：

- 仓库中的 `/tasks` 接口只是**执行任务**
- 它不是 WBS 业务任务模型

结论：

- 这部分本来就不属于 agent 范围
- 当前设计与该需求并不冲突

## 6.2 需求 2：从任务清单发起本地 Git 操作

需求内容：

- clone 项目代码
- pull 项目代码
- 设定选中的分支为当前分支
- 提交 merge request 到目标分支

评估结论：

- **大部分已支持**
- merge request 创建本身**不支持**，且应该保留在上层

详细映射如下：

- clone 项目代码
  - 由 `git.clone` 支持

- pull 项目代码
  - 由 `git.pull` 支持

- 将选中的分支设为当前分支
  - 由 `git.checkout_branch` 支持

- 从当前分支向目标分支提交 merge request
  - 当前不直接支持
  - 当前 agent 没有 GitLab/GitHub/Gitea API 集成
  - 应由上层系统在本地分支 push 之后，通过托管平台 API 完成

需要明确区分：

- `git.merge_branch` 是**本地 merge**
- 它**不是**远端 merge request / pull request 行为

结论：

- 当前本地 Git 控制能力足够
- MR/PR 创建不应该强行塞进这个 agent

## 6.3 需求 3：“开发当前任务”

需求内容：

- clone 项目代码
- 切换到所需分支
- 从当前分支创建新分支
- 分支名使用任务 ID + 任务简述

评估结论：

- **可以通过上层编排 + 当前 agent 动作实现**

所需动作：

1. `git.status`
   - 获取当前分支和工作区状态

2. `git.checkout_branch`
   - 切换到选中的基础分支

3. `git.checkout_branch` 配合 `create = true`
   - 基于当前 HEAD 或指定基础分支创建任务分支

职责分工：

- 上层系统决定分支命名规则
- agent 只负责实际执行 Git 操作

结论：

- 当前 agent 足以支撑这条流程
- 不需要把任务领域逻辑塞到 agent 中

## 6.4 需求 4：“commit 当前代码”并在 commit 前做 LLM 审核

需求内容：

- 对当前任务代码进行 add 和 commit
- commit 前运行 LLM 审核
- 判断 staged 代码是否满足任务要求
- 审核通过后才允许 commit

评估结论：

- **可以通过上层编排实现**
- **当前不支持把审核决策内建在 agent 里**

之所以可接受，是因为：

- agent 已经提供 `git.add`
- agent 已经提供 `git.commit`
- agent 已经提供 `command.run`
- 上层可以通过 `command.run` 读取 staged diff
- 上层可以独立调用 LLM
- 上层决定是否继续调用 `git.commit`

推荐编排方式：

1. 调用 `git.add`
2. 调用 `command.run` 执行 `git diff --cached`
3. 把任务详情和 staged diff 传给 LLM
4. 审核通过后调用 `git.commit`
5. 不通过则由上层提示并停止 commit

结论：

- 当前 agent 足够支撑该流程
- LLM 审核应保留在本地执行层之外

---

## 7. 端到端能力映射

| 用户动作 | 当前 agent 支持情况 | 适配结论 | 说明 |
| --- | --- | --- | --- |
| 查看分配的 WBS 任务 | 否 | 超出范围 | 由上层系统负责 |
| clone 项目代码 | 是 | 良好 | `git.clone` |
| pull 项目代码 | 是 | 良好 | `git.pull` |
| 将选中分支设为当前分支 | 是 | 良好 | `git.checkout_branch` |
| 查看当前分支和仓库状态 | 是 | 良好 | `git.status` |
| 查看分支列表 | 是 | 良好 | 现已提供 `git.list_branches_structured`，适合 UI 分支选择 |
| 开发当前任务 | 是 | 良好 | 上层通过 `status` + 创建分支来编排 |
| 暂存代码 | 是 | 良好 | `git.add` |
| 获取 staged diff 供 LLM 审核 | 是 | 良好 | 已支持 `git.diff_staged`，也可回退到 `command.run` |
| commit 当前任务代码 | 是 | 良好 | `git.commit` |
| push 任务分支 | 是 | 良好 | `git.push` |
| 创建 merge request / pull request | 否 | 超出范围 | 由上层调用托管平台 API |

整体结论：

- 如果把它定位成 Git-only 的本地 agent，当前项目在功能上**已经基本够用**

---

## 8. 推荐的上层编排方式

## 8.1 clone 项目代码

上层系统保存：

- `repo_url`
- `workspace_root`
- `local_repo_name`

上层系统计算：

- `destination_path = workspace_root + "/" + local_repo_name`

Agent 调用示例：

```json
{
  "action": "git.clone",
  "params": {
    "remote_url": "git@gitlab.example.com:team/app.git",
    "destination_path": "/Users/dev/workspace/app"
  }
}
```

## 8.2 pull 项目代码

Agent 调用示例：

```json
{
  "action": "git.pull",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "remote": "origin",
    "branch": "main",
    "rebase": false
  }
}
```

推荐的上层预检查：

- 先调用 `git.status`
- 如果本地存在未提交修改，则先提示用户再决定是否继续 pull

## 8.3 将选中的分支设为当前分支

Agent 调用示例：

```json
{
  "action": "git.checkout_branch",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "branch_name": "release/1.2.0",
    "create": false
  }
}
```

## 8.4 开发当前任务

推荐流程：

1. 调用 `git.status`
2. 获取 `current_branch`
3. 上层根据任务 ID + 任务简述生成分支名
4. 基于当前分支或指定基础分支创建任务分支

Agent 调用示例：

```json
{
  "action": "git.checkout_branch",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "branch_name": "task/REQ-102-fix-login-cache",
    "create": true,
    "start_point": "main"
  }
}
```

## 8.5 commit 当前代码并在 commit 前执行 LLM 审核

推荐流程：

1. 暂存文件
2. 获取 staged diff
3. 由上层执行 LLM 审核
4. 审核通过后才 commit

暂存指定文件：

```json
{
  "action": "git.add",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "paths": ["src/auth.rs", "tests/auth_flow.rs"]
  }
}
```

或暂存全部修改：

```json
{
  "action": "git.add",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "all": true
  }
}
```

读取 staged diff：

```json
{
  "action": "command.run",
  "params": {
    "program": "git",
    "args": ["diff", "--cached"],
    "cwd": "/Users/dev/workspace/app"
  }
}
```

读取 staged 文件列表：

```json
{
  "action": "command.run",
  "params": {
    "program": "git",
    "args": ["diff", "--cached", "--name-only"],
    "cwd": "/Users/dev/workspace/app"
  }
}
```

审核通过后执行 commit：

```json
{
  "action": "git.commit",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "message": "feat: complete REQ-102 login cache handling"
  }
}
```

## 8.6 提交 merge request

推荐流程：

1. push 本地任务分支
2. 上层调用 Git 托管平台 API
3. 在远端创建 MR / PR

Push 调用示例：

```json
{
  "action": "git.push",
  "params": {
    "repo_path": "/Users/dev/workspace/app",
    "remote": "origin",
    "branch": "task/REQ-102-fix-login-cache",
    "set_upstream": true
  }
}
```

重要说明：

- 不要把 `git.merge_branch` 当作 merge request 的替代方案
- `git.merge_branch` 只表示本地 merge

---

## 9. 实际运行约束与假设

当前设计之所以足够，是建立在以下前提下：

1. 开发机已经安装 Git。
2. 开发机已经配置好可用的 Git 凭证。
3. 仓库已经配置 `user.name` 和 `user.email`，或者开发机全局 Git 身份已配置。
4. 上层系统能正确保存并传入工作区路径。
5. 工作区路径始终位于 `AGENT_ALLOWED_ROOTS` 之内。
6. 上层系统负责 MR / PR 创建。
7. 上层系统负责 LLM 审核。

---

## 10. 当前缺口与风险

以下问题不会阻塞当前 Git-only 的角色定位，但仍然值得关注。

## 10.1 WebSocket 路径配置曾存在历史不一致问题

这个问题现在已经修复：配置项 `AGENT_WS_PATH` 会被规范化，并且会实际注册为 WebSocket 路由。

当前行为：

- 默认仍然是 `/ws`
- 自定义路径现在会被真正生效

剩余注意事项：

- 上层系统应使用实际配置的 WebSocket 路径，而不是固定假设 `/ws`

严重性：

- 已解决

## 10.2 路径白名单校验目前是词法级别，不解析符号链接

当前路径校验会先做路径归一化，但在授权前不会解析符号链接。

影响：

- 允许根目录下如果存在指向外部目录的 symlink，理论上可能绕过目录边界

严重性：

- 从安全视角看属于中到高

## 10.3 旧的 `git.list_branches` 仍然返回纯文本

为了兼容旧调用方式，原有 `git.list_branches` 仍保持文本输出，但现在已经新增了结构化版本。

影响：

- 老的集成方式可能仍然解析文本输出
- 新的集成方式应优先使用 `git.list_branches_structured`

严重性：

- 低

## 10.4 进程输出当前没有显式大小限制

当前进程执行器会把完整 stdout/stderr 收集到内存中。

影响：

- 如果命令输出过大，会造成不必要的内存增长

严重性：

- 中

## 10.5 失败路径测试覆盖仍然偏窄

当前集成测试覆盖了主成功路径，但以下失败场景尚未覆盖：

- 未授权访问
- 非法路径拒绝
- pull 冲突
- push 被拒绝
- Git 身份未配置导致 commit 失败
- 超时行为

严重性：

- 中

---

## 11. 建议的小幅增强

以下上层接入便捷动作现在已经实现：

- `git.get_current_branch`
- `git.diff_staged`
- `git.list_branches_structured`

当前仍然建议继续处理的事项：

### 11.1 加强路径校验，支持 symlink 感知

目的：

- 让路径策略真正基于文件系统边界，而不是仅做词法路径比较

---

## 12. 最终结论

如果目标收敛为：

- 在开发机上提供一个本地 Git 执行端点
- 由上层项目管理系统调用
- 更高层的业务流程和 LLM 审核都放在外部实现

那么当前 `cross-platform-agent` 项目**已经基本足够**。

它已经支持上层需要的核心执行链路：

- clone
- pull
- checkout branch
- create branch
- add
- 通过 `git.diff_staged` 或 `command.run` 获取 staged diff
- commit
- push

它**不支持**，而且**也不应该被要求支持**以下职责：

- WBS 任务管理
- 指派和登录逻辑
- merge request / pull request 创建
- LLM 推理和审核决策流程

这些职责应继续保留在上层系统中。

最终建议：

- 保持这个 agent 足够薄
- 把它严格当作受控的本地 Git 执行网关
- 由上层负责业务编排和模型审核
- 后续如有需要，再继续增加少量便于接入的 Git 查询接口

