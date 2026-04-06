use anyhow::{anyhow, Context, Result};
use serde_json::Value;

/// Represents a parsed Anthropic messages API response.
#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicMessageResponse {
    pub id: Option<String>,
    pub model: Option<String>,
    pub role: Option<String>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub content: Vec<AnthropicContentBlock>,
}

impl AnthropicMessageResponse {
    /// Parses an Anthropic response from a JSON string.
    pub fn parse_json(payload: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(payload)?;
        Self::from_value(value)
    }

    /// Parses an Anthropic response from a raw JSON value.
    pub fn from_value(value: Value) -> Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("Anthropic response must be a JSON object"))?;
        let content = object
            .get("content")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(Self {
            id: object
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            model: object
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            role: object
                .get("role")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            stop_reason: object
                .get("stop_reason")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            stop_sequence: object
                .get("stop_sequence")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            content: parse_content_blocks(content)?,
        })
    }

    /// Returns all parsed text blocks in the order Anthropic returned them.
    pub fn text_blocks(&self) -> Vec<&AnthropicTextBlock> {
        self.content
            .iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    /// Returns all parsed tool-use blocks in the order Anthropic returned them.
    pub fn tool_uses(&self) -> Vec<&AnthropicToolUseBlock> {
        self.content
            .iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::ToolUse(tool_use) => Some(tool_use),
                _ => None,
            })
            .collect()
    }
}

/// Represents one content block from an Anthropic response.
#[derive(Debug, Clone, PartialEq)]
pub enum AnthropicContentBlock {
    Text(AnthropicTextBlock),
    ToolUse(AnthropicToolUseBlock),
    Other(AnthropicUnknownBlock),
}

/// Represents a text block from an Anthropic response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicTextBlock {
    pub text: String,
}

/// Represents a tool-use block from an Anthropic response.
#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicToolUseBlock {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Represents an unsupported or currently unmodeled Anthropic content block.
#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicUnknownBlock {
    pub block_type: String,
    pub raw: Value,
}

fn parse_content_blocks(value: Value) -> Result<Vec<AnthropicContentBlock>> {
    let blocks = value
        .as_array()
        .ok_or_else(|| anyhow!("Anthropic response content must be an array"))?;
    blocks
        .iter()
        .cloned()
        .map(parse_content_block)
        .collect::<Result<Vec<_>>>()
}

fn parse_content_block(value: Value) -> Result<AnthropicContentBlock> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Anthropic content block must be a JSON object"))?;
    let block_type = object
        .get("type")
        .and_then(Value::as_str)
        .context("Anthropic content block did not include a string type")?;
    match block_type {
        "text" => Ok(AnthropicContentBlock::Text(AnthropicTextBlock {
            text: object
                .get("text")
                .and_then(Value::as_str)
                .context("Anthropic text block did not include text")?
                .to_string(),
        })),
        "tool_use" => Ok(AnthropicContentBlock::ToolUse(AnthropicToolUseBlock {
            id: object
                .get("id")
                .and_then(Value::as_str)
                .context("Anthropic tool_use block did not include id")?
                .to_string(),
            name: object
                .get("name")
                .and_then(Value::as_str)
                .context("Anthropic tool_use block did not include name")?
                .to_string(),
            input: object
                .get("input")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default())),
        })),
        _ => Ok(AnthropicContentBlock::Other(AnthropicUnknownBlock {
            block_type: block_type.to_string(),
            raw: Value::Object(object.clone()),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_text_only_response() {
        let response = AnthropicMessageResponse::parse_json(
            r#"{
                "id": "msg_1",
                "model": "claude-sonnet-4-5",
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "hello there"}
                ],
                "stop_reason": "end_turn"
            }"#,
        )
        .unwrap();

        assert_eq!(response.text_blocks().len(), 1);
        assert_eq!(response.text_blocks()[0].text, "hello there");
        assert!(response.tool_uses().is_empty());
    }

    #[test]
    fn parses_mixed_text_and_tool_use_blocks() {
        let response = AnthropicMessageResponse::from_value(json!({
            "id": "msg_2",
            "model": "claude-sonnet-4-5",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Running bash now."},
                {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "bash",
                    "input": {"command": "pwd"}
                }
            ],
            "stop_reason": "tool_use"
        }))
        .unwrap();

        assert_eq!(response.text_blocks()[0].text, "Running bash now.");
        assert_eq!(response.tool_uses().len(), 1);
        assert_eq!(response.tool_uses()[0].id, "toolu_1");
        assert_eq!(response.tool_uses()[0].name, "bash");
        assert_eq!(response.tool_uses()[0].input["command"], "pwd");
    }

    #[test]
    fn preserves_unknown_blocks_for_future_handling() {
        let response = AnthropicMessageResponse::from_value(json!({
            "content": [
                {"type": "thinking", "thinking": "hidden"}
            ]
        }))
        .unwrap();

        assert!(matches!(
            response.content[0],
            AnthropicContentBlock::Other(AnthropicUnknownBlock { .. })
        ));
    }
}
