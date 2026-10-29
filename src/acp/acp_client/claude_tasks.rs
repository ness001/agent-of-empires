//! Native Claude subagent and async-task updates not yet in the Rust ACP schema.

use std::collections::HashMap;

use agent_client_protocol::schema::v1::{SessionNotification, SessionUpdate};
use serde_json::Value;

use crate::acp::state::{
    AsyncTaskRecord, BackgroundAgentStatus, BackgroundAgentTool, Event, SessionGoal,
};

pub(super) const NATIVE_UPDATE_KEY: &str = "agentOfEmpires.nativeUpdate";

/// Keep extension updates inside the typed admission path until ACP models them.
pub(super) fn decode_notification(
    mut params: Value,
) -> Result<SessionNotification, serde_json::Error> {
    if matches!(
        params
            .pointer("/update/sessionUpdate")
            .and_then(Value::as_str),
        Some(
            "subagent_spawned"
                | "subagent_state_update"
                | "async_task_spawned"
                | "async_task_progress"
                | "async_task_state_update"
        )
    ) {
        let update = params["update"].take();
        params["update"] = serde_json::json!({
            "sessionUpdate": "session_info_update",
            "_meta": { NATIVE_UPDATE_KEY: update },
        });
    }
    serde_json::from_value(params)
}

pub(super) fn native_update(update: &SessionUpdate) -> Option<&Value> {
    let SessionUpdate::SessionInfoUpdate(info) = update else {
        return None;
    };
    info.meta.as_ref()?.get(NATIVE_UPDATE_KEY)
}

pub(super) fn goal_event(update: &SessionUpdate) -> Option<Event> {
    let SessionUpdate::SessionInfoUpdate(info) = update else {
        return None;
    };
    let goal = info
        .meta
        .as_ref()?
        .get("jetbrains")?
        .get("air")?
        .get("goal")?;
    if goal.is_null() {
        return Some(Event::GoalUpdated { goal: None });
    }
    Some(Event::GoalUpdated {
        goal: Some(SessionGoal {
            objective: goal.get("objective")?.as_str()?.to_owned(),
            status: goal.get("status")?.as_str()?.to_owned(),
            iterations: goal.get("iterations").and_then(Value::as_u64),
            last_reason: string(goal, "lastReason"),
            created_at: goal
                .get("createdAt")
                .filter(|v| v.is_number() || v.is_string())
                .cloned(),
        }),
    })
}

#[derive(Default)]
pub(super) struct ClaudeTaskTracker {
    children: HashMap<String, Child>,
    tasks: HashMap<String, AsyncTaskRecord>,
}

#[derive(Default)]
struct Child {
    tools: HashMap<String, BackgroundAgentTool>,
    text: String,
    ended: bool,
}

impl ClaudeTaskTracker {
    pub(super) fn clear(&mut self) {
        self.children.clear();
        self.tasks.clear();
    }

    /// Child output never mutates the main turn's tool, thinking or stop state.
    pub(super) fn observe(&mut self, notification: &SessionNotification) -> Option<Vec<Event>> {
        if let Some(raw) = native_update(&notification.update) {
            let kind = raw.get("sessionUpdate")?.as_str()?;
            if kind.starts_with("async_task_") {
                return Some(self.async_task(kind, raw));
            }
            let id = string(raw, "subagentSessionId")?;
            if kind == "subagent_spawned" {
                if self.children.contains_key(&id) {
                    return Some(Vec::new());
                }
                self.children.insert(id.clone(), Child::default());
                return Some(vec![Event::BackgroundAgentLaunched {
                    agent_id: id,
                    tool_call_id: String::new(),
                    description: string(raw, "name").unwrap_or_default(),
                    prompt: string(raw, "prompt")
                        .or_else(|| string(raw, "task"))
                        .unwrap_or_default(),
                    model: String::new(),
                    wire_tracked: true,
                    output_file: String::new(),
                    started_at: chrono::Utc::now(),
                }]);
            }
            let Some(status) =
                raw.get("state")
                    .and_then(Value::as_str)
                    .and_then(|state| match state {
                        "completed" => Some(BackgroundAgentStatus::Completed),
                        "failed" => Some(BackgroundAgentStatus::Error),
                        "cancelled" | "disconnected" => Some(BackgroundAgentStatus::Detached),
                        _ => None,
                    })
            else {
                return Some(Vec::new());
            };
            let Some(child) = self.children.get_mut(&id) else {
                return Some(Vec::new());
            };
            if child.ended {
                return Some(Vec::new());
            }
            child.ended = true;
            return Some(vec![Event::BackgroundAgentCompleted {
                agent_id: id,
                status,
                tools: child_tools(child),
                result: (!child.text.is_empty()).then(|| child.text.clone()),
                warning: None,
                ended_at: chrono::Utc::now(),
            }]);
        }
        let id = notification.session_id.0.as_ref();
        let child = self.children.get_mut(id)?;
        if child.ended {
            return Some(Vec::new());
        }
        let mut last_tool = None;
        match &notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let agent_client_protocol::schema::v1::ContentBlock::Text(text) = &chunk.content
                {
                    child.text.push_str(&text.text);
                    // The panel stores a bounded preview, not a second transcript.
                    if child.text.len() > 16_384 {
                        let mut start = child.text.len() - 16_384;
                        while !child.text.is_char_boundary(start) {
                            start += 1;
                        }
                        child.text.drain(..start);
                    }
                }
            }
            SessionUpdate::ToolCall(tool) => {
                last_tool = Some(tool.title.clone());
                child.tools.insert(
                    tool.tool_call_id.to_string(),
                    BackgroundAgentTool {
                        name: tool.title.clone(),
                        title: None,
                        ok: None,
                    },
                );
            }
            SessionUpdate::ToolCallUpdate(tool) => {
                if let Some(existing) = child.tools.get_mut(&tool.tool_call_id.to_string()) {
                    if let Some(title) = &tool.fields.title {
                        existing.name = title.clone();
                    }
                    use agent_client_protocol::schema::v1::ToolCallStatus;
                    existing.ok = match tool.fields.status {
                        Some(ToolCallStatus::Completed) => Some(true),
                        Some(ToolCallStatus::Failed) => Some(false),
                        _ => existing.ok,
                    };
                    last_tool = Some(existing.name.clone());
                }
            }
            _ => return Some(Vec::new()),
        }
        Some(vec![Event::BackgroundAgentProgress {
            agent_id: id.to_owned(),
            status: BackgroundAgentStatus::Running,
            tool_count: child.tools.len().min(u32::MAX as usize) as u32,
            tools: child_tools(child),
            last_tool,
            last_text: (!child.text.is_empty()).then(|| child.text.clone()),
            model: None,
            at: chrono::Utc::now(),
        }])
    }

    fn async_task(&mut self, kind: &str, raw: &Value) -> Vec<Event> {
        let Some(id) = string(raw, "asyncTaskId") else {
            return Vec::new();
        };
        if kind == "async_task_spawned" {
            self.tasks
                .entry(id.clone())
                .or_insert_with(|| AsyncTaskRecord {
                    id: id.clone(),
                    name: string(raw, "name").unwrap_or_default(),
                    task_type: string(raw, "taskType").unwrap_or_default(),
                    description: string(raw, "description").unwrap_or_default(),
                    state: "running".into(),
                    show_in_transcript: raw
                        .get("showInTranscript")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    can_stop: raw.get("canStop").and_then(Value::as_bool).unwrap_or(false),
                    tool_call_id: string(raw, "toolCallId"),
                    summary: None,
                    last_tool: None,
                    usage: None,
                });
        }
        let Some(task) = self.tasks.get_mut(&id) else {
            return Vec::new();
        };
        if kind == "async_task_progress" && !task.is_active() {
            return Vec::new();
        }
        if let Some(state) = string(raw, "state") {
            if !matches!(
                state.as_str(),
                "running" | "paused" | "completed" | "failed" | "stopped"
            ) {
                return Vec::new();
            }
            if !task.is_active() && matches!(state.as_str(), "running" | "paused") {
                return Vec::new();
            }
            task.state = state;
            task.can_stop &= task.is_active();
        }
        if let Some(description) = string(raw, "description") {
            task.description = description;
        }
        if let Some(summary) = string(raw, "summary") {
            task.summary = Some(summary);
        }
        if let Some(last_tool) = string(raw, "lastToolName") {
            task.last_tool = Some(last_tool);
        }
        if let Some(tool_call_id) = string(raw, "toolCallId") {
            task.tool_call_id = Some(tool_call_id);
        }
        if let Some(usage) = raw.get("usage").filter(|v| v.is_object()) {
            task.usage = Some(usage.clone());
        }
        vec![Event::AsyncTaskUpdated { task: task.clone() }]
    }
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::state::{AcpSessionId, AcpState, AgentName};
    use serde_json::json;

    fn notification(session: &str, update: Value) -> SessionNotification {
        decode_notification(json!({"sessionId": session, "update": update})).unwrap()
    }

    #[test]
    fn native_lifecycle_stays_separate_from_main_turn_and_replays() {
        let mut tracker = ClaudeTaskTracker::default();
        let mut state = AcpState::new(AcpSessionId("s".into()), AgentName("claude".into()), None);
        let updates = [
            notification(
                "root",
                json!({"sessionUpdate":"subagent_spawned","subagentSessionId":"child","name":"Review","task":"Check code","capabilities":{}}),
            ),
            notification(
                "child",
                json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"Read"}),
            ),
            notification(
                "child",
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Result"}}),
            ),
            notification(
                "root",
                json!({"sessionUpdate":"subagent_state_update","subagentSessionId":"child","state":"completed"}),
            ),
            notification(
                "child",
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"late"}}),
            ),
        ];
        for update in updates {
            for event in tracker.observe(&update).unwrap() {
                let event: Event =
                    serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
                state.apply_event(state.last_seq + 1, event).unwrap();
            }
            assert!(!state.turn_active);
            assert!(state.in_flight_tool.is_none());
            assert!(state.thinking.is_none());
        }
        let child = &state.background_agents[0];
        assert_eq!(child.status, BackgroundAgentStatus::Completed);
        assert_eq!(child.tool_count, 1);
        assert_eq!(child.result.as_deref(), Some("Result"));
        assert!(!state.has_active_background_agent());
        state
            .apply_event(state.last_seq + 1, Event::SessionCleared)
            .unwrap();
        assert!(state.background_agents.is_empty());
    }

    #[test]
    fn goal_and_workflow_wire_preserve_patches_and_terminal_state() {
        let goal = |value| {
            notification(
                "root",
                json!({"sessionUpdate":"session_info_update","_meta":{"jetbrains":{"air":{"version":1,"goal":value}}}}),
            )
        };
        let mut state = AcpState::default();
        let set = goal(
            json!({"objective":"Finish","status":"active","iterations":2,"lastReason":"retry","createdAt":123}),
        );
        state
            .apply_event(1, goal_event(&set.update).unwrap())
            .unwrap();
        assert_eq!(state.goal.as_ref().unwrap().iterations, Some(2));
        let clear = goal(Value::Null);
        state
            .apply_event(2, goal_event(&clear.update).unwrap())
            .unwrap();
        assert!(state.goal.is_none());
        assert!(goal_event(
            &notification(
                "root",
                json!({"sessionUpdate":"session_info_update","title":"not a goal"})
            )
            .update
        )
        .is_none());

        let mut tracker = ClaudeTaskTracker::default();
        for update in [
            json!({"sessionUpdate":"async_task_spawned","asyncTaskId":"wf","name":"Build","taskType":"workflow","description":"Compile","showInTranscript":true,"canStop":true}),
            json!({"sessionUpdate":"async_task_progress","asyncTaskId":"wf","summary":"Checking","toolCallId":"t","outputFilePath":"/private/output"}),
            json!({"sessionUpdate":"async_task_state_update","asyncTaskId":"wf","state":"completed","summary":"Done"}),
            json!({"sessionUpdate":"async_task_progress","asyncTaskId":"wf","summary":"late"}),
        ] {
            for event in tracker.observe(&notification("root", update)).unwrap() {
                assert!(!serde_json::to_string(&event)
                    .unwrap()
                    .contains("/private/output"));
                state.apply_event(state.last_seq + 1, event).unwrap();
            }
        }
        assert_eq!(state.async_tasks.len(), 1);
        let task = &state.async_tasks[0];
        assert_eq!(task.state, "completed");
        assert_eq!(task.summary.as_deref(), Some("Done"));
        assert_eq!(task.tool_call_id.as_deref(), Some("t"));
        assert!(!task.can_stop);
        assert!(!state.turn_active);
        state
            .apply_event(state.last_seq + 1, Event::SessionCleared)
            .unwrap();
        assert!(state.async_tasks.is_empty());
    }
}

fn child_tools(child: &Child) -> Vec<BackgroundAgentTool> {
    let mut tools: Vec<_> = child.tools.iter().collect();
    tools.sort_by_key(|(id, _)| *id);
    tools.into_iter().map(|(_, tool)| tool.clone()).collect()
}
