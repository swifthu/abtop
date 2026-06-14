# abtop — minimax 模型 context window 按名映射 设计

**日期**: 2026-06-14
**状态**: Draft
**作用域**: 单文件 15 行 + 4 个测试

## 1. 目标

让 abtop 的 Claude Code 监控能正确显示 minimax 模型的 context window 上限（不再依赖 token 数启发式猜 200K/1M），支持：
- `MiniMax-M3` 系列 → 512K
- `MiniMax-*[1M]` 后缀 → 1M
- `MiniMax-M2.7` 系列 → 200K
- 未知模型 → fallback 到原启发式

**核心约束**（来自用户澄清）：
- minimax 模型都在 Claude Code 上跑（`api.minimaxi.com/anthropic/v1/messages` 走 Anthropic Messages API 兼容协议）
- minimax API response **没有** self-reported context_window 字段
- 用户不接受"实测"或"抓包"路径，要求**纯模型名后缀硬映射**
- 未知模型 fallback 到 200K

## 2. 改动文件

| 路径 | 变更 |
|---|---|
| `src/collector/claude.rs` | 改 `context_window_for_model()` 函数 + 扩展 `test_context_window_for_model()` 测试 |

**不改**：`app.rs`、`ui/quota.rs`、`model/session.rs`、其他 collector。`AgentSession.context_window` 字段已存在，UI 已正确渲染。**只是更新它的赋值算法**。

## 3. 优先级链

```
context_window_for_model(transcript_model, configured_model, max_context_tokens)
    │
    │  合并两字符串（大小写不敏感）
    ▼
combined = lowercase(transcript_model) + " " + lowercase(configured_model)
    │
    ├─ 1. combined.contains("[1m]")    → 1_000_000   // 显式 1M 标记（含 `MiniMax-M3[1M]`）优先于 M3 基线
    ├─ 2. combined.contains("m3")       → 512_000
    ├─ 3. combined.contains("m2.7")    → 200_000
    └─ 4. fallback: max > 200K → 1M else 200K
```

**`[1M]` 优先级最高**的理由：
- `[1M]` 是用户显式声明的 1M 上下文请求（如 `MiniMax-M3[1M]`），优先级应高于 M3 基线 512K
- 与原启发式中 `[1m]` 优先于 max_token 阈值的逻辑保持一致
- 副作用：未来 `M3-pro[1M]` 也会被识别为 1M（与 M3-pro 实际规格对齐）

## 4. 实现

```rust
fn context_window_for_model(transcript_model: &str, configured_model: &str, max_context_tokens: u64) -> u64 {
    // 合并两个模型名字段（transcript + settings.json 配置），任一含子串即可
    let combined = format!(
        "{} {}",
        transcript_model.to_ascii_lowercase(),
        configured_model.to_ascii_lowercase()
    );

    // minimax 模型名精确匹配（顺序重要：[1M] 比 M3 优先）
    if combined.contains("[1m]") {
        return 1_000_000;
    }
    if combined.contains("m3") {
        return 512_000;
    }
    if combined.contains("m2.7") {
        return 200_000;
    }

    // Fallback: 原启发式（基于观测到的 max token 数）
    if max_context_tokens > 200_000 {
        1_000_000
    } else {
        200_000
    }
}
```

**为什么不用 `to_ascii_lowercase` 别名闭包**：
- 当前 1 处使用，inline 写比抽闭包可读
- 2 次调用，每次分配一个 short String，可忽略

## 5. 测试

扩展 `test_context_window_for_model`（位于 `src/collector/claude.rs:3090`）：

### 5.1 保留旧测试（fallback 路径不变）

```rust
assert_eq!(context_window_for_model("claude-opus-4-6", "", 50_000), 200_000);
assert_eq!(context_window_for_model("claude-opus-4-6[1m]", "", 0), 1_000_000);
assert_eq!(context_window_for_model("claude-sonnet-4-6", "sonnet[1m]", 0), 1_000_000);
assert_eq!(context_window_for_model("claude-sonnet-4-6", "", 100_000), 200_000);
assert_eq!(context_window_for_model("unknown-model", "", 0), 200_000);
assert_eq!(context_window_for_model("claude-opus-4-6", "", 250_000), 1_000_000);
```

### 5.2 新增 minimax 映射测试

```rust
// M3 → 512K
assert_eq!(context_window_for_model("MiniMax-M3", "", 0), 512_000);
assert_eq!(context_window_for_model("MiniMax-M3-highspeed", "", 100_000), 512_000);
assert_eq!(context_window_for_model("claude-opus-4-6", "MiniMax-M3", 0), 512_000);
// 大小写不敏感
assert_eq!(context_window_for_model("minimax-m3", "", 0), 512_000);
assert_eq!(context_window_for_model("MINIMAX-M3", "", 0), 512_000);

// [1M] 后缀 → 1M（`[1M]` 优先于 M3 基线，所以 `MiniMax-M3[1M]` 走 1M）
assert_eq!(context_window_for_model("MiniMax-M3[1M]", "", 0), 1_000_000);
assert_eq!(context_window_for_model("MiniMax-something[1M]", "", 0), 1_000_000);
assert_eq!(context_window_for_model("claude-opus-4-6[1M]", "", 0), 1_000_000);

// M2.7 → 200K
assert_eq!(context_window_for_model("MiniMax-M2.7", "", 0), 200_000);
assert_eq!(context_window_for_model("minimax-m2.7-highspeed", "", 0), 200_000);

// 跨字段匹配：transcript 不含、configured 含也应命中
assert_eq!(context_window_for_model("", "MiniMax-M3", 0), 512_000);
assert_eq!(context_window_for_model("", "MiniMax-M2.7", 0), 200_000);
```

## 6. 边界与权衡

| 场景 | 行为 |
|---|---|
| 已知 minimax 模型 (`M3`/`[1M]`/`M2.7`) | 精确映射，context_percent 计算正确 |
| 未知 minimax 模型（未来 `M4`/`M5`） | 走 fallback 启发式（可能错 200K/1M） — YAGNI，出现时加一行 |
| 已知 Claude 模型 | 不变（fallback 路径） |
| 未知 Claude 模型 | 不变（200K） |
| 模型名 `m2.7` 子串误匹配（比如 `m2.71`） | 风险存在但低（minimax 当前无此命名） |

**风险**：`combined.contains("m3")` 也会匹配 `m30`、`m300` 等未来模型。当前 minimax 命名无此冲突。

## 7. 不做的（YAGNI）

- ❌ 不加 `~/.config/abtop/config.toml` 里的 `[model_window]` 表让用户自定义映射
- ❌ 不抽公共 `model_window_lookup()` 到 `src/model/`（claude/codex 各自维护）
- ❌ 不改 Codex 的 turn_context 路径（Codex 已经有 self-reported 路径，模型名映射对它无意义）
- ❌ 不加 `M4`/`M5` 等未来模型名预留（出现时加一行）
- ❌ 不动 `AgentSession.context_window` 字段类型（u64 够用，1M < u64::MAX）

## 8. 文件清单

| 路径 | 类型 | 预计行数 |
|---|---|---|
| `src/collector/claude.rs` | 修改（`context_window_for_model` 函数体 + 测试扩展） | +30 行（实现 ~12 + 测试 ~18） |

总改动：~30 行，单文件，零新依赖，零新模块。

## 9. 风险与回滚

- **风险 1**：`m3` 子串误匹配 — 实际无冲突
- **回滚**：revert commit 即可，恢复到原启发式

## 10. 开放问题

无。
