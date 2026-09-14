//! 上下文分项:输入框下方的上下文圆环点开后看到的分项占用(2026-09-14,
//! 计划 `docs/plan-is-true/2026-09-14/context-panel.md`)。
//!
//! 只从真实请求的同一份字节里拆:消息走 `chat_messages("", "")`,工具走
//! `ToolRegistry::presented_definitions`(`definitions` / `stub_definitions`
//! 也从它出)。各分项之和必须等于 `context_tokens_estimate()`,测试钉着——另写
//! 一套渲染、或漏归一类消息,那里先报红。
//!
//! 供应商报的实测总数拆不开,只整数给出;实测与估算之差由前端单列成「分词器
//! 差异」,这里不按比例摊进各分项——那是编数字。

use crate::agent::*;
use crate::llm::{ChatContent, ChatContentPart, ChatMessage, ToolDefinition};
use crate::tools::PresentedToolKind;
use serde::Serialize;
use std::collections::HashMap;
use std::ops::Range;

const TOP_ITEMS: usize = 5;
const PREVIEW_CHARS: usize = 60;
/// `tool_report::summary_checkpoint_message` 的外壳,压缩摘要行靠它认。
const CHECKPOINT_PREFIX: &str = "<conversation-checkpoint>";
const SKILL_TOOL: &str = "load_skill";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    System,
    Skills,
    Summary,
    Fossil,
    Messages,
}

impl Category {
    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Skills => "skills",
            Self::Summary => "summary",
            Self::Fossil => "fossil",
            Self::Messages => "messages",
        }
    }
}

/// 各分项的 o200k 估算。字段名就是前端的分项键。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ContextCategories {
    /// system 消息与预设对话。
    pub system: u64,
    /// 完整 schema 发出的工具(full 模式全部、stub 模式的常驻工具)。
    pub tools_full: u64,
    /// stub 模式下懒工具的「真名 + 摘要 + 宽松参数壳」。
    pub tools_stub: u64,
    /// MCP 服务器登记的工具,不论以哪种形态发出。
    pub mcp: u64,
    /// `load_skill` 的结果。
    pub skills: u64,
    /// 压缩摘要行。
    pub summary: u64,
    /// 化石化回放的瞬态尾巴(runtime、联想记忆、提醒等)。
    pub fossil: u64,
    /// 其余历史:用户消息、她的回复、工具调用与结果。
    pub messages: u64,
}

impl ContextCategories {
    pub fn total(&self) -> u64 {
        self.system
            + self.tools_full
            + self.tools_stub
            + self.mcp
            + self.skills
            + self.summary
            + self.fossil
            + self.messages
    }

    fn add(&mut self, category: Category, tokens: u64) {
        let slot = match category {
            Category::System => &mut self.system,
            Category::Skills => &mut self.skills,
            Category::Summary => &mut self.summary,
            Category::Fossil => &mut self.fossil,
            Category::Messages => &mut self.messages,
        };
        *slot += tokens;
    }
}

/// 占用最多的单条消息。它是分项内部的明细,不参与合计。
#[derive(Debug, Clone, Serialize)]
pub struct ContextTopItem {
    /// `tool_result` | `tool_call` | `assistant` | `user`
    pub kind: &'static str,
    /// 工具名(工具调用与结果);其余为空。
    pub label: String,
    pub preview: String,
    /// 所属分项的键,与 `ContextCategories` 字段同名。
    pub category: &'static str,
    pub tokens: u64,
    /// 第几个可见历史回合(1 起);对不上号时为空,不猜。
    pub turn_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextBreakdown {
    pub categories: ContextCategories,
    /// 各分项之和,等于 `context_tokens_estimate()`。
    pub estimate_tokens: u64,
    /// 上一回合最后一次请求的供应商实测占用;没有锚点时为空。
    pub measured_tokens: Option<u64>,
    /// stub 模式下还没展开的完整契约。不在上下文里。
    pub deferred_tools_tokens: u64,
    pub top: Vec<ContextTopItem>,
}

impl Agent {
    pub fn context_breakdown(&self) -> Result<ContextBreakdown> {
        let (messages, user_index) = self.chat_messages("", "")?;
        let owners = self.history_turn_owners(&messages, user_index)?;
        let preset_end = 1 + self.preset_dialogs.len() * 2;

        let mut categories = ContextCategories::default();
        let mut tool_names: HashMap<&str, &str> = HashMap::new();
        let mut candidates = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            for call in message.tool_calls.iter().flatten() {
                tool_names.insert(call.id.as_str(), call.function.name.as_str());
            }
            let tokens = overflow::message_tokens(message) as u64;
            let tool_name = message
                .tool_call_id
                .as_deref()
                .and_then(|id| tool_names.get(id).copied());
            let category = if index < preset_end {
                Category::System
            } else if message.transient_context {
                Category::Fossil
            } else if message.role == "tool" && tool_name == Some(SKILL_TOOL) {
                Category::Skills
            } else if message.role == "user" && text_of(message).starts_with(CHECKPOINT_PREFIX) {
                Category::Summary
            } else {
                Category::Messages
            };
            categories.add(category, tokens);
            if matches!(category, Category::Messages | Category::Skills) && index != user_index {
                candidates.push(top_item(
                    message,
                    category,
                    tokens,
                    owners[index],
                    tool_name,
                ));
            }
        }

        let mut deferred_tools_tokens = 0;
        if self.tools_enabled {
            let tools = self.tools.lock().unwrap();
            let stub_mode = crate::tools::is_stub_loading_mode(
                &crate::tools::effective_tools_loading_mode(&self.config),
            );
            let mut full = Vec::<ToolDefinition>::new();
            let mut stub = Vec::new();
            let mut mcp = Vec::new();
            for presented in tools.presented_definitions(stub_mode) {
                match presented.kind {
                    PresentedToolKind::Full => full.push(presented.definition),
                    PresentedToolKind::Stub => stub.push(presented.definition),
                    PresentedToolKind::Mcp => mcp.push(presented.definition),
                }
            }
            categories.tools_full = estimate_tool_definition_tokens(&full) as u64;
            categories.tools_stub = estimate_tool_definition_tokens(&stub) as u64;
            categories.mcp = estimate_tool_definition_tokens(&mcp) as u64;
            if stub_mode {
                deferred_tools_tokens =
                    estimate_tool_definition_tokens(&tools.deferred_contract_definitions()) as u64;
            }
        }

        candidates.sort_by(|a: &ContextTopItem, b| b.tokens.cmp(&a.tokens));
        candidates.truncate(TOP_ITEMS);
        let estimate_tokens = categories.total();
        Ok(ContextBreakdown {
            categories,
            estimate_tokens,
            measured_tokens: self.context_anchor_tokens()?,
            deferred_tools_tokens,
            top: candidates,
        })
    }

    /// 每条消息属于第几个可见历史回合(1 起)。用 `chat_messages` 同一个
    /// `push_history_turn` 重渲一遍来对位;历史段对不上「当前用户消息之前那一截」
    /// 时整张表留空——宁可不标,不猜。
    fn history_turn_owners(
        &self,
        messages: &[ChatMessage],
        user_index: usize,
    ) -> Result<Vec<Option<usize>>> {
        let mut owners = vec![None; messages.len()];
        if self.suppress_session_history {
            return Ok(owners);
        }
        let turns = self.state.load_visible_turns_excluding("")?;
        let mut rendered = Vec::new();
        let mut spans: Vec<(Range<usize>, usize)> = Vec::new();
        let mut ordinal = 0usize;
        for turn in &turns {
            if turn.is_summary || turn.status == crate::state::TurnStatus::Running {
                continue;
            }
            ordinal += 1;
            let start = rendered.len();
            self.push_history_turn(&mut rendered, turn);
            spans.push((start..rendered.len(), ordinal));
        }
        let Some(history_start) = user_index.checked_sub(rendered.len()) else {
            return Ok(owners);
        };
        let aligned = rendered
            .iter()
            .zip(&messages[history_start..user_index])
            .all(|(a, b)| a.role == b.role && a.tool_call_id == b.tool_call_id);
        if !aligned {
            return Ok(owners);
        }
        for (range, ordinal) in spans {
            for index in range {
                owners[history_start + index] = Some(ordinal);
            }
        }
        Ok(owners)
    }
}

fn top_item(
    message: &ChatMessage,
    category: Category,
    tokens: u64,
    turn_index: Option<usize>,
    tool_name: Option<&str>,
) -> ContextTopItem {
    let text = text_of(message);
    let calls = message.tool_calls.as_deref().unwrap_or_default();
    let (kind, label, preview_source) = match message.role.as_str() {
        "tool" => (
            "tool_result",
            tool_name.unwrap_or_default().to_string(),
            text,
        ),
        "assistant" if text.trim().is_empty() && !calls.is_empty() => (
            "tool_call",
            calls
                .iter()
                .map(|call| call.function.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            calls[0].function.arguments.as_str(),
        ),
        "assistant" => ("assistant", String::new(), text),
        _ => ("user", String::new(), text),
    };
    ContextTopItem {
        kind,
        label,
        preview: preview(preview_source),
        category: category.key(),
        tokens,
        turn_index,
    }
}

fn text_of(message: &ChatMessage) -> &str {
    match &message.content {
        Some(ChatContent::Text(text)) => text,
        Some(ChatContent::Parts(parts)) => parts
            .iter()
            .find_map(|part| match part {
                ChatContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or_default(),
        None => "",
    }
}

fn preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = collapsed.chars().take(PREVIEW_CHARS).collect::<String>();
    if collapsed.chars().count() > PREVIEW_CHARS {
        out.push('…');
    }
    out
}
