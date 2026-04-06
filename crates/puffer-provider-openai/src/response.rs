use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A minimal OpenAI Responses API payload needed by the runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIResponsesResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub output_text: Option<String>,
    #[serde(default)]
    pub output: Vec<OpenAIResponsesOutputItem>,
}

/// A single output item from the OpenAI Responses API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIResponsesOutputItem {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub content: Vec<OpenAIResponsesContentItem>,
}

/// A content fragment nested under an OpenAI assistant message output item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAIResponsesContentItem {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
}

/// A parsed tool call emitted by the OpenAI Responses API.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAIResponseToolCall {
    pub item_id: Option<String>,
    pub status: Option<String>,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

/// Parses a serialized OpenAI Responses API payload.
pub(crate) fn parse_responses_response(payload: &str) -> Result<OpenAIResponsesResponse> {
    serde_json::from_str(payload).context("failed to parse OpenAI Responses payload")
}

/// Extracts assistant text from a parsed OpenAI Responses payload.
pub(crate) fn extract_responses_text(response: &OpenAIResponsesResponse) -> String {
    if let Some(text) = response.output_text.as_ref() {
        if !text.trim().is_empty() {
            return text.clone();
        }
    }

    response
        .output
        .iter()
        .filter(|item| item.kind == "message")
        .flat_map(|item| item.content.iter())
        .filter(|item| item.kind == "output_text")
        .filter_map(|item| item.text.as_deref())
        .collect::<Vec<_>>()
        .join("")
}

/// Extracts tool calls from a parsed OpenAI Responses payload.
pub(crate) fn extract_responses_tool_calls(
    response: &OpenAIResponsesResponse,
) -> Result<Vec<OpenAIResponseToolCall>> {
    response
        .output
        .iter()
        .filter(|item| item.kind == "function_call")
        .map(parse_tool_call)
        .collect()
}

fn parse_tool_call(item: &OpenAIResponsesOutputItem) -> Result<OpenAIResponseToolCall> {
    let call_id = item
        .call_id
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| anyhow!("OpenAI tool call item is missing call_id"))?;
    let name = item
        .name
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| anyhow!("OpenAI tool call item is missing name"))?;
    let raw_arguments = item
        .arguments
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| anyhow!("OpenAI tool call item is missing arguments"))?;
    let arguments = serde_json::from_str(&raw_arguments)
        .with_context(|| format!("failed to parse OpenAI tool arguments for call {call_id}"))?;

    Ok(OpenAIResponseToolCall {
        item_id: item.id.clone(),
        status: item.status.clone(),
        call_id,
        name,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_text_from_message_content() {
        let response = parse_responses_response(
            r#"{
                "id": "resp_123",
                "model": "gpt-5",
                "output": [
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            { "type": "output_text", "text": "alpha " },
                            { "type": "output_text", "text": "beta" }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(extract_responses_text(&response), "alpha beta");
        assert!(extract_responses_tool_calls(&response).unwrap().is_empty());
    }

    #[test]
    fn prefers_top_level_output_text_when_present() {
        let response = parse_responses_response(
            r#"{
                "id": "resp_123",
                "output_text": "top-level text",
                "output": [
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            { "type": "output_text", "text": "nested text" }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(extract_responses_text(&response), "top-level text");
    }

    #[test]
    fn extracts_function_calls_from_output_items() {
        let response = parse_responses_response(
            r#"{
                "id": "resp_123",
                "output": [
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            { "type": "output_text", "text": "Let me inspect that." }
                        ]
                    },
                    {
                        "type": "function_call",
                        "id": "fc_123",
                        "status": "completed",
                        "call_id": "call_123",
                        "name": "read_file",
                        "arguments": "{\"path\":\"Cargo.toml\"}"
                    }
                ]
            }"#,
        )
        .unwrap();

        let calls = extract_responses_tool_calls(&response).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].item_id.as_deref(), Some("fc_123"));
        assert_eq!(calls[0].status.as_deref(), Some("completed"));
        assert_eq!(calls[0].call_id, "call_123");
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments, json!({ "path": "Cargo.toml" }));
    }

    #[test]
    fn rejects_invalid_tool_argument_json() {
        let response = parse_responses_response(
            r#"{
                "output": [
                    {
                        "type": "function_call",
                        "call_id": "call_123",
                        "name": "read_file",
                        "arguments": "{not-json}"
                    }
                ]
            }"#,
        )
        .unwrap();

        let error = extract_responses_tool_calls(&response).unwrap_err();
        assert!(error.to_string().contains("call_123"));
    }
}
