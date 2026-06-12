# Skill 行动义务守卫设计

- 日期：2026-06-12
- 状态：设计复审完成，待实现
- 范围：agent runtime + skill frontmatter；不重建短剧 internal tool

## 1. 背景与问题

`short-drama` 已改成一份薄 skill：agent 读取 `SKILL.md` 后，自己编排
剧本、分镜、`imagegen`、`videogen` 和 `ffmpeg`。这符合长期方向：短剧制作尽量由
skill 完成，Rust 端只提供原子能力与最小执行约束。

回归点在 runtime：当前 agent loop 遇到无工具调用的 assistant turn 会直接结束：

- `crates/puffer-core/runtime/agent_loop.rs` 的 no-tool 分支把纯文本当最终答案。
- `crates/puffer-core/runtime/blocking_loop.rs` 有同构 no-tool 分支。
- 旧 `short_drama_obligation.rs` 已不存在；新 `short-drama` skill 也没有 runtime 守卫。

结果是弱模型可以在读取 skill 后只回复“我会开始生成...”这类承诺文本，loop 会把它当成
成功交付。短剧 skill 文案里已有“promise-only 不算完成”，但这是软约束，不能可靠约束
模型行为。

## 2. 目标 / 非目标

**目标**

- 给 agentic skill 一个通用、声明式的“必须开始行动”能力。
- 防止声明该能力的 skill 被 promise-only 文本误判为完成。
- 保持短剧仍是薄 skill，不恢复胖 short-drama internal tool。
- 同时覆盖 streaming agent loop 与 blocking loop。
- 默认关闭；未声明的 skill 行为不变。

**非目标**

- 不判断任务是否真正完成，只判断 skill 触发后是否开始工具驱动的工作。
- 不引入 skill workflow DSL、阶段图、完成条件语言或 provider 级强制 `tool_choice`。
- 不做 short-drama 专用 hack。
- 不为旧 `short-drama-generation` 兼容旧 key 或旧 `shortdrama --brief` 判定。
- 不通过模型路由规避问题；路由只能作为临时运营策略，不是本设计的一部分。

## 3. 方案概述

新增一个通用的 **Skill Action Obligation** 机制：

1. skill frontmatter 可声明：

   ```yaml
   requires-action: true
   ```

2. runtime 观察已执行的工具结果，而不是仅观察模型请求。
3. 当 `Skill` 工具成功执行、且进入了一个 `requires-action: true` 的 skill 后，
   runtime 记录一个 pending obligation。
4. 后续出现任意真实工具调用时，obligation 满足。
5. 如果后续模型返回无工具调用：
   - 第一次：runtime 注入纠正提醒并继续下一轮。
   - 第二次：runtime 返回明确失败文本，禁止 promise-only 成功退出。

该机制是 stop-hook style 的对话层兜底，而不是请求层 `tool_choice=required`。原因是：

- 跨 Anthropic、OpenAI 兼容接口和其它 provider 更稳定。
- 不改变 provider 请求构造，不增加工具选择副作用。
- 额外成本只发生在模型违规时，正常路径无额外 LLM 回合。

## 4. Skill 声明模型

在 `puffer-resources::SkillSpec` 新增字段：

```rust
pub requires_action: bool
```

serde / loader 接受两个 frontmatter key：

- `requires-action`
- `requiresAction`

默认值为 `false`。

`resources/skills/short-drama/SKILL.md` 增加：

```yaml
requires-action: true
```

语义：模型触发该 skill 后，后续必须至少发起一个工具调用，才算已经开始执行。触发 skill
本身不算开始执行，因为 `Skill` 调用只读取说明，不产生用户请求的工作产物。

## 5. Runtime 状态机

新增模块：

```text
crates/puffer-core/runtime/skill_obligation.rs
```

核心结构：

```rust
pub(crate) struct SkillActionObligation { ... }

pub(crate) enum NoToolDecision {
    Complete,
    ContinueWithReminder(String),
    FailNotStarted(String),
}
```

核心 API：

```rust
impl SkillActionObligation {
    pub(crate) fn observe_invocations(
        &mut self,
        resources: &LoadedResources,
        invocations: &[ToolInvocation],
    );

    pub(crate) fn no_tool_decision(&mut self) -> NoToolDecision;
}
```

状态：

- `Idle`：没有待履行义务。
- `Pending { skill_name, reminders_sent }`：声明了行动义务的 skill 已触发，但还没看到后续真实工具调用。
- 满足后回到 `Idle`。不需要永久 `Satisfied` 状态，因为守卫只约束当前 pending skill。

事件规则：

- 观察到成功的 `ToolInvocation { tool_id: "Skill", success: true, ... }`：
  通过 `Skill` 工具共享 helper 从 invocation input 解析被激活的 skill 名，再用
  `skill_by_name(resources, name)` 查出 canonical `SkillSpec`。如果目标 skill 的
  `requires_action == true`，设置 pending obligation。
- 失败的 `Skill` invocation 不触发 obligation。unknown skill、`disable-model-invocation`
  拒绝、权限拒绝等错误不应被误判为“已进入 skill”。
- 观察到 pending 状态下的任意非 `Skill` invocation：
  清空 pending obligation。
- 观察到 pending 状态下再次触发另一个 `requires-action` skill：
  用最新 skill 覆盖 pending。这样避免多 pending 队列和复杂优先级，符合实际 agent
  一次进入一个 agentic skill 的使用方式。
- 模型返回无工具调用：
  - 无 pending：`Complete`。
  - pending 且 `reminders_sent == 0`：`ContinueWithReminder` 并递增计数。
  - pending 且已提醒过：`FailNotStarted`。

## 6. Loop 集成点

### Streaming loop

在 `crates/puffer-core/runtime/agent_loop.rs`：

1. agent loop 开始时创建 `SkillActionObligation`。
2. 在现有 `turn.tool_calls.is_empty()` 分支中，先问：

   ```rust
   match obligation.no_tool_decision() { ... }
   ```

3. `Complete` 走现有 final assistant text 路径。
4. `ContinueWithReminder` 必须先把 `turn.pre_tool_items` 追加进 conversation items，
   保留模型刚才的 promise/progress 文本；然后追加一条 user reminder，再 `continue`
   下一轮。这样下一次 provider 调用能看到“刚才为什么被纠正”。
5. `FailNotStarted` 返回一段明确失败 assistant text，并运行与普通 final text 一致的收尾逻辑。
6. 工具批执行完成并得到 `new_invocations` 后，调用：

   ```rust
   obligation.observe_invocations(inputs.resources, &new_invocations);
   ```

   这一步放在 FunctionCallOutput 入 conversation 前后均可；它只影响后续 no-tool
   分支，不改变本批工具结果。

### Blocking loop

在 `crates/puffer-core/runtime/blocking_loop.rs` 做同样接入，避免非 streaming 路径绕过守卫。

## 7. 提醒与失败文本

提醒文本应该短、具体、禁止承诺式回复：

```text
The "{skill_name}" skill was activated, but no follow-up tool call has started its required work.
Do not answer with a promise, plan, or progress update.
Your next response must either call an appropriate tool to begin the work, or state the concrete blocker that makes execution impossible.
```

失败文本：

```text
The "{skill_name}" skill was activated, but the model did not start the required tool-driven work after a reminder. No work was started.
```

失败作为 assistant text 返回，而不是 runtime error。原因是模型没有履行任务，不是程序执行崩溃；
用户需要看到清晰原因，session 也应保留这个可解释终态。

## 8. 数据流

正常路径：

```text
User asks for short drama
-> model calls Skill(short-drama)
-> Skill invocation succeeds and returns the skill prompt
-> obligation pending
-> model calls Write/Bash/Read/etc.
-> obligation satisfied
-> loop proceeds normally
```

违规后恢复路径：

```text
User asks for short drama
-> model calls Skill(short-drama)
-> Skill invocation succeeds
-> obligation pending
-> model returns pure promise text
-> runtime injects reminder, does not finish
-> model calls Write/Bash/etc.
-> obligation satisfied
```

违规失败路径：

```text
User asks for short drama
-> model calls Skill(short-drama)
-> Skill invocation succeeds
-> obligation pending
-> model returns pure promise text
-> runtime injects reminder
-> model again returns no tool calls
-> runtime returns FailNotStarted assistant text
```

## 9. 稳定性与性能

稳定性：

- 约束点在 runtime no-tool 分支，正好覆盖 promise-only 的干净退出路径。
- obligation 只由成功执行后的 `Skill` invocation 激活，避免把 unknown/disabled skill
  的失败结果误判为 pending。
- streaming 与 blocking loop 一致接入，避免路径差异。
- 不依赖模型自觉遵守 skill 文案。
- 不依赖特定 provider 的 tool-choice 功能。

性能：

- 正常合规路径只做一次内存状态判断，无额外模型调用。
- 只有违规 promise-only 时才增加最多一轮 LLM 重试。
- 状态为单 pending，不做队列、图调度或复杂完成判定。

## 10. 测试计划

`puffer-resources`：

- loader 能解析 `requires-action: true`。
- 未声明时 `SkillSpec.requires_action == false`。
- serde alias `requiresAction` 可用。

`puffer-core` obligation 单元测试：

- 成功的 `Skill(short-drama)` invocation 后无工具调用 -> `ContinueWithReminder`。
- 提醒后仍无工具调用 -> `FailNotStarted`。
- 成功的 `Skill(short-drama)` invocation 后出现任意非 `Skill` invocation -> `Complete`。
- 失败的 `Skill(short-drama)` invocation 不触发 pending。
- 未声明 `requires-action` 的 skill 不触发 pending。
- pending 中触发另一个 requires-action skill 时，pending skill 名更新。
- 同一批 invocation 中 `Skill(short-drama)` 后跟 `Write` -> pending 立即满足。

loop 集成测试：

- streaming loop：第一轮 `Skill(short-drama)`，第二轮纯 promise，第三轮 `Write`；
  断言第二轮没有结束，最终包含 `Write` invocation。
- streaming loop：提醒后仍纯文本，断言返回 failure assistant text。
- blocking loop：至少覆盖 promise 后续轮，不允许直接从 no-tool 分支返回成功。

短剧资源测试：

- `resources/skills/short-drama/SKILL.md` frontmatter 含 `requires-action: true`。
- 保留 `allowed-tools`、`user-invocable`、`disable-model-invocation` 既有语义。

## 11. 受影响文件

实现预计修改：

- `crates/puffer-resources/src/model.rs`
- `crates/puffer-resources/src/loader.rs`
- `crates/puffer-core/runtime.rs`
- `crates/puffer-core/runtime/skill_obligation.rs`
- `crates/puffer-core/runtime/claude_tools/skill.rs`
- `crates/puffer-core/runtime/agent_loop.rs`
- `crates/puffer-core/runtime/blocking_loop.rs`
- `resources/skills/short-drama/SKILL.md`
- 对应测试文件
- `specs/puffer-core/` 下一个未使用编号的组件更新 spec
- `specs/puffer-resources/` 下一个未使用编号的组件更新 spec

## 12. 取舍记录

采用通用 frontmatter 声明，而不是 short-drama 专用守卫：

- 短剧暴露的是 agentic skill 的通用失败模式，不是短剧特例。
- 一个布尔声明足以覆盖问题，不需要 DSL。
- 后续其它多步 skill 可按需启用。

采用提醒续轮，而不是 `tool_choice=required`：

- 更跨 provider。
- 不改变请求层工具选择策略。
- 与旧 short-drama 守卫的成功经验一致。

基于成功 invocation 激活，而不是基于模型请求激活：

- 复用 `Skill` 工具自己的 unknown/disabled/lambda gate 校验结果。
- 不在 obligation 守卫里复制 permission、resource lookup 或 provider 请求逻辑。
- 不需要给所有 runtime-local tool 改返回类型，也不需要新增复杂 metadata 管道。

只判定“开始”，不判定“完成”：

- 这正好修复 promise-only 回归。
- 完成判定需要任务语义、产物 schema 和 workflow 状态，会把 runtime 推向 workflow engine。
- 短剧后续质量仍由 skill 文案、工具返回和验收提示控制。
