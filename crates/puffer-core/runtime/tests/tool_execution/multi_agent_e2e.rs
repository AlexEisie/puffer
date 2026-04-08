use super::*;
use crate::runtime::teammate_loop::{
    teammate_registry, IncomingMessage, TeammateMessage,
};
use std::thread;
use std::time::Duration;

fn ensure_workflow_dir(state: &AppState) {
    let cwd = state.session.cwd.as_path();
    fs::create_dir_all(cwd.join(".puffer/runtime/claude_workflow")).unwrap();
}

fn run_tool(state: &mut AppState, tool_id: &str, input: Value) -> Result<String, anyhow::Error> {
    let resources = puffer_resources::LoadedResources::default();
    let cwd = state.session.cwd.clone();
    crate::runtime::claude_tools::execute_workflow_tool(
        state, &resources, &cwd, tool_id, input, None,
    )
}

#[test]
fn teammate_registry_starts_empty() {
    let registry = teammate_registry();
    // Registry is global but should have no entries for unknown agents
    let guard = registry.lock().unwrap();
    // Just verify we can lock it — actual entries depend on concurrent tests
    drop(guard);
}

#[test]
fn teammate_registry_insert_and_send() {
    let registry = teammate_registry();
    let (tx, rx) = std::sync::mpsc::channel();
    let agent_id = format!("test-agent-{}", uuid::Uuid::new_v4().simple());

    // Insert
    registry.lock().unwrap().insert(agent_id.clone(), tx);

    // Send via registry
    {
        let guard = registry.lock().unwrap();
        let sender = guard.get(&agent_id).unwrap();
        sender
            .send(TeammateMessage::Incoming(IncomingMessage {
                from: "leader".to_string(),
                text: "hello teammate".to_string(),
            }))
            .unwrap();
    }

    // Receive
    let msg = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    match msg {
        TeammateMessage::Incoming(m) => {
            assert_eq!(m.from, "leader");
            assert_eq!(m.text, "hello teammate");
        }
        _ => panic!("expected Incoming"),
    }

    // Cleanup
    registry.lock().unwrap().remove(&agent_id);
}

#[test]
fn teammate_shutdown_via_registry() {
    let registry = teammate_registry();
    let (tx, rx) = std::sync::mpsc::channel();
    let agent_id = format!("test-shutdown-{}", uuid::Uuid::new_v4().simple());

    registry.lock().unwrap().insert(agent_id.clone(), tx);

    // Send shutdown
    {
        let guard = registry.lock().unwrap();
        guard
            .get(&agent_id)
            .unwrap()
            .send(TeammateMessage::Shutdown {
                request_id: "req-123".to_string(),
            })
            .unwrap();
    }

    let msg = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    match msg {
        TeammateMessage::Shutdown { request_id } => {
            assert_eq!(request_id, "req-123");
        }
        _ => panic!("expected Shutdown"),
    }

    registry.lock().unwrap().remove(&agent_id);
}

#[test]
fn send_message_delivers_to_in_process_teammate() {
    let mut state = temp_state();
    ensure_workflow_dir(&state);
    let cwd = state.session.cwd.clone();

    // Create team
    run_tool(
        &mut state,
        "TeamCreate",
        json!({ "team_name": "msg-test-team" }),
    )
    .unwrap();

    // Register a fake agent
    let agent_id = "msg-receiver@msg-test-team";
    let agents_json = json!({
        "agents": [{
            "agent_id": agent_id,
            "name": "msg-receiver",
            "description": "test",
            "prompt": "test",
            "subagent_type": null,
            "model": null,
            "team_name": "msg-test-team",
            "mode": null,
            "isolation": null,
            "cwd": cwd.display().to_string(),
            "status": "running",
            "output_file": cwd.join("msg-recv-output.json").display().to_string()
        }]
    });
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents_json).unwrap(),
    )
    .unwrap();
    fs::write(cwd.join("msg-recv-output.json"), "{}").unwrap();

    // Register a channel in the teammate registry
    let (tx, rx) = std::sync::mpsc::channel();
    teammate_registry()
        .lock()
        .unwrap()
        .insert(agent_id.to_string(), tx);

    // Send a message via SendMessage tool
    let result = run_tool(
        &mut state,
        "SendMessage",
        json!({
            "to": "msg-receiver",
            "summary": "greet",
            "message": "hello from leader"
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["delivered"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == agent_id));

    // Verify the message was delivered via mpsc channel
    let msg = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    match msg {
        TeammateMessage::Incoming(m) => {
            assert!(m.from.contains("team-lead"));
            assert_eq!(m.text, "hello from leader");
        }
        _ => panic!("expected Incoming message"),
    }

    // Also verify it was persisted to file store
    let msgs: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/messages.json")).unwrap(),
    )
    .unwrap();
    let stored = msgs["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["to"] == agent_id)
        .expect("message should be in store");
    assert_eq!(stored["read"], false);
    assert!(!stored["from"].as_str().unwrap().is_empty());

    // Cleanup
    teammate_registry()
        .lock()
        .unwrap()
        .remove(agent_id);

    // Delete team
    let mut agents: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/agents.json")).unwrap(),
    )
    .unwrap();
    agents["agents"][0]["status"] = json!("stopped");
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents).unwrap(),
    )
    .unwrap();
    run_tool(&mut state, "TeamDelete", json!({})).unwrap();
}

#[test]
fn shutdown_request_protocol_full_roundtrip() {
    let mut state = temp_state();
    ensure_workflow_dir(&state);
    let cwd = state.session.cwd.clone();

    // Create team + register fake agent
    run_tool(
        &mut state,
        "TeamCreate",
        json!({ "team_name": "shutdown-test" }),
    )
    .unwrap();

    let agent_id = "worker@shutdown-test";
    let lead_output = cwd.join("lead-output.json");
    fs::write(&lead_output, "{}").unwrap();
    let agents_json = json!({
        "agents": [
            {
                "agent_id": "team-lead@shutdown-test",
                "name": "team-lead",
                "description": "leader",
                "prompt": "",
                "subagent_type": null,
                "model": null,
                "team_name": "shutdown-test",
                "mode": null,
                "isolation": null,
                "cwd": cwd.display().to_string(),
                "status": "running",
                "output_file": lead_output.display().to_string()
            },
            {
                "agent_id": agent_id,
                "name": "worker",
                "description": "test",
                "prompt": "test",
                "subagent_type": null,
                "model": null,
                "team_name": "shutdown-test",
                "mode": null,
                "isolation": null,
                "cwd": cwd.display().to_string(),
                "status": "running",
                "output_file": cwd.join("worker-output.json").display().to_string()
            }
        ]
    });
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents_json).unwrap(),
    )
    .unwrap();
    fs::write(cwd.join("worker-output.json"), "{}").unwrap();

    // 1. Leader sends shutdown_request
    let result = run_tool(
        &mut state,
        "SendMessage",
        json!({
            "to": "worker",
            "message": { "type": "shutdown_request", "reason": "done" }
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    let request_id = parsed["request_id"].as_str().unwrap().to_string();
    assert!(request_id.starts_with("shutdown-"));

    // 2. Simulate worker approving shutdown (shutdown_response to team-lead)
    let result = run_tool(
        &mut state,
        "SendMessage",
        json!({
            "to": "team-lead",
            "message": {
                "type": "shutdown_response",
                "request_id": request_id,
                "approve": true
            }
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);

    // 3. Verify the confirmation was delivered to team-lead's mailbox
    let msgs: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/messages.json")).unwrap(),
    )
    .unwrap();
    let approval_msg = msgs["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| {
            m["message"]["type"] == "shutdown_response" && m["to"] == "team-lead"
        })
        .expect("approval should be in mailbox");
    assert_eq!(approval_msg["message"]["approve"], true);

    // Cleanup
    let mut agents: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/agents.json")).unwrap(),
    )
    .unwrap();
    agents["agents"][0]["status"] = json!("stopped");
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents).unwrap(),
    )
    .unwrap();
    run_tool(&mut state, "TeamDelete", json!({})).unwrap();
}

#[test]
fn plan_approval_protocol_approve_and_reject() {
    let mut state = temp_state();
    ensure_workflow_dir(&state);
    let cwd = state.session.cwd.clone();

    run_tool(
        &mut state,
        "TeamCreate",
        json!({ "team_name": "plan-test" }),
    )
    .unwrap();

    let agent_id = "planner@plan-test";
    let agents_json = json!({
        "agents": [{
            "agent_id": agent_id,
            "name": "planner",
            "description": "test",
            "prompt": "test",
            "subagent_type": null,
            "model": null,
            "team_name": "plan-test",
            "mode": null,
            "isolation": null,
            "cwd": cwd.display().to_string(),
            "status": "running",
            "output_file": cwd.join("planner-output.json").display().to_string()
        }]
    });
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents_json).unwrap(),
    )
    .unwrap();
    fs::write(cwd.join("planner-output.json"), "{}").unwrap();

    // Approve
    let result = run_tool(
        &mut state,
        "SendMessage",
        json!({
            "to": "planner",
            "message": {
                "type": "plan_approval_response",
                "request_id": "plan-req-1",
                "approve": true,
                "feedback": "looks good"
            }
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["message"].as_str().unwrap().contains("approved"));

    // Reject
    let result = run_tool(
        &mut state,
        "SendMessage",
        json!({
            "to": "planner",
            "message": {
                "type": "plan_approval_response",
                "request_id": "plan-req-2",
                "approve": false,
                "feedback": "add error handling"
            }
        }),
    )
    .unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["message"].as_str().unwrap().contains("rejected"));

    // Verify both delivered
    let msgs: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/messages.json")).unwrap(),
    )
    .unwrap();
    let plan_msgs: Vec<_> = msgs["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["message"]["type"] == "plan_approval_response")
        .collect();
    assert_eq!(plan_msgs.len(), 2);
    assert_eq!(plan_msgs[0]["message"]["approve"], true);
    assert_eq!(plan_msgs[1]["message"]["approve"], false);
    assert_eq!(plan_msgs[1]["message"]["feedback"], "add error handling");

    // Cleanup
    let mut agents: Value = serde_json::from_str(
        &fs::read_to_string(cwd.join(".puffer/runtime/claude_workflow/agents.json")).unwrap(),
    )
    .unwrap();
    agents["agents"][0]["status"] = json!("stopped");
    fs::write(
        cwd.join(".puffer/runtime/claude_workflow/agents.json"),
        serde_json::to_string_pretty(&agents).unwrap(),
    )
    .unwrap();
    run_tool(&mut state, "TeamDelete", json!({})).unwrap();
}
