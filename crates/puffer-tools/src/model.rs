use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Describes the primitive type for a tool schema node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSchemaType {
    Object,
    String,
}

impl ToolSchemaType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::String => "string",
        }
    }
}

/// Defines one named property inside a tool input schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPropertySchema {
    pub value_type: ToolSchemaType,
    pub description: String,
    pub required: bool,
}

/// Describes the top-level structured input expected by a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolInputSchema {
    pub properties: BTreeMap<String, ToolPropertySchema>,
}

impl ToolInputSchema {
    /// Converts the typed schema into a JSON Schema object for model APIs.
    pub fn as_json_schema(&self) -> Value {
        let properties = self
            .properties
            .iter()
            .map(|(name, property)| {
                (
                    name.clone(),
                    json!({
                        "type": property.value_type.as_str(),
                        "description": property.description,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>();
        let required = self
            .properties
            .iter()
            .filter_map(|(name, property)| property.required.then(|| name.clone()))
            .collect::<Vec<_>>();
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    }
}

/// Captures practical runtime metadata for a local workspace tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolMetadata {
    pub may_spawn_processes: bool,
    pub may_read_files: bool,
    pub may_write_files: bool,
}

/// Carries optional approval and sandbox hints for a tool definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolPolicyHints {
    pub approval_policy: Option<String>,
    pub sandbox_policy: Option<String>,
}

/// Identifies one built-in local tool implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Bash,
    ReadFile,
    WriteFile,
}

impl ToolKind {
    /// Returns the built-in tool kind for a handler id when it is supported.
    pub fn from_handler(handler: &str) -> Option<Self> {
        match handler {
            "bash" => Some(Self::Bash),
            "read_file" => Some(Self::ReadFile),
            "write_file" => Some(Self::WriteFile),
            _ => None,
        }
    }

    /// Returns the canonical built-in handler id for the tool kind.
    pub fn handler(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
        }
    }

    /// Returns the canonical built-in tool definition for this tool kind.
    pub fn definition(self) -> ToolDefinition {
        builtin_tool_definition(self)
    }
}

/// Full typed tool definition used by the registry and model adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub handler: String,
    pub kind: ToolKind,
    pub input_schema: ToolInputSchema,
    pub metadata: ToolMetadata,
    pub policy: ToolPolicyHints,
}

impl ToolDefinition {
    /// Converts the tool definition into an Anthropic-compatible tool payload.
    pub fn as_anthropic_tool(&self) -> Value {
        json!({
            "name": self.id,
            "description": self.description,
            "input_schema": self.input_schema.as_json_schema(),
        })
    }
}

/// Typed input for the built-in bash tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BashToolInput {
    pub command: String,
}

/// Typed input for the built-in read-file tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadFileToolInput {
    pub path: PathBuf,
}

/// Typed input for the built-in write-file tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteFileToolInput {
    pub path: PathBuf,
    pub contents: String,
}

/// Tagged tool input accepted by the tool CLI and model tool loops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolInput {
    Bash { command: String },
    ReadFile { path: PathBuf },
    WriteFile { path: PathBuf, contents: String },
}

impl ToolInput {
    /// Returns the built-in tool kind represented by this input payload.
    pub fn kind(&self) -> ToolKind {
        match self {
            Self::Bash { .. } => ToolKind::Bash,
            Self::ReadFile { .. } => ToolKind::ReadFile,
            Self::WriteFile { .. } => ToolKind::WriteFile,
        }
    }

    /// Converts the enum into the matching kind plus strongly typed payload.
    pub fn into_kind_payload(self) -> (ToolKind, TypedToolInput) {
        match self {
            Self::Bash { command } => (
                ToolKind::Bash,
                TypedToolInput::Bash(BashToolInput { command }),
            ),
            Self::ReadFile { path } => (
                ToolKind::ReadFile,
                TypedToolInput::ReadFile(ReadFileToolInput { path }),
            ),
            Self::WriteFile { path, contents } => (
                ToolKind::WriteFile,
                TypedToolInput::WriteFile(WriteFileToolInput { path, contents }),
            ),
        }
    }
}

/// Strongly typed tool payloads used by local executors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedToolInput {
    Bash(BashToolInput),
    ReadFile(ReadFileToolInput),
    WriteFile(WriteFileToolInput),
}

/// Captures normalized stdout, stderr, and metadata from one tool run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub metadata: Value,
}

/// Stores the normalized result of one local tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub tool_id: String,
    pub success: bool,
    pub output: ToolOutput,
}

/// Returns all built-in tool definitions in stable id order.
pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        builtin_tool_definition(ToolKind::Bash),
        builtin_tool_definition(ToolKind::ReadFile),
        builtin_tool_definition(ToolKind::WriteFile),
    ]
}

/// Returns the built-in tool definition for the given tool kind.
pub fn builtin_tool_definition(kind: ToolKind) -> ToolDefinition {
    match kind {
        ToolKind::Bash => ToolDefinition {
            id: "bash".to_string(),
            name: "bash".to_string(),
            description: "Run a shell command in the current workspace.".to_string(),
            handler: kind.handler().to_string(),
            kind,
            input_schema: ToolInputSchema {
                properties: BTreeMap::from([(
                    "command".to_string(),
                    ToolPropertySchema {
                        value_type: ToolSchemaType::String,
                        description: "Shell command to execute.".to_string(),
                        required: true,
                    },
                )]),
            },
            metadata: ToolMetadata {
                may_spawn_processes: true,
                may_read_files: true,
                may_write_files: true,
            },
            policy: ToolPolicyHints::default(),
        },
        ToolKind::ReadFile => ToolDefinition {
            id: "read_file".to_string(),
            name: "read_file".to_string(),
            description: "Read one file from the current workspace.".to_string(),
            handler: kind.handler().to_string(),
            kind,
            input_schema: ToolInputSchema {
                properties: BTreeMap::from([(
                    "path".to_string(),
                    ToolPropertySchema {
                        value_type: ToolSchemaType::String,
                        description: "Path to the file to read.".to_string(),
                        required: true,
                    },
                )]),
            },
            metadata: ToolMetadata {
                may_spawn_processes: false,
                may_read_files: true,
                may_write_files: false,
            },
            policy: ToolPolicyHints::default(),
        },
        ToolKind::WriteFile => ToolDefinition {
            id: "write_file".to_string(),
            name: "write_file".to_string(),
            description: "Write one file in the current workspace.".to_string(),
            handler: kind.handler().to_string(),
            kind,
            input_schema: ToolInputSchema {
                properties: BTreeMap::from([
                    (
                        "path".to_string(),
                        ToolPropertySchema {
                            value_type: ToolSchemaType::String,
                            description: "Path to the file to write.".to_string(),
                            required: true,
                        },
                    ),
                    (
                        "contents".to_string(),
                        ToolPropertySchema {
                            value_type: ToolSchemaType::String,
                            description: "New file contents.".to_string(),
                            required: true,
                        },
                    ),
                ]),
            },
            metadata: ToolMetadata {
                may_spawn_processes: false,
                may_read_files: false,
                may_write_files: true,
            },
            policy: ToolPolicyHints::default(),
        },
    }
}

/// Returns the built-in tool definition for the given handler id, if available.
pub fn builtin_tool_definition_by_handler(handler: &str) -> Option<ToolDefinition> {
    match handler {
        "bash" => Some(builtin_tool_definition(ToolKind::Bash)),
        "read_file" => Some(builtin_tool_definition(ToolKind::ReadFile)),
        "write_file" => Some(builtin_tool_definition(ToolKind::WriteFile)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_definitions_cover_all_supported_tools() {
        let definitions = builtin_tool_definitions();
        let ids = definitions
            .iter()
            .map(|definition| definition.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["bash", "read_file", "write_file"]);
    }

    #[test]
    fn write_file_schema_is_exported_for_models() {
        let definition = builtin_tool_definition(ToolKind::WriteFile);
        let schema = definition.input_schema.as_json_schema();
        let required = schema.get("required").and_then(Value::as_array).unwrap();
        assert_eq!(required.len(), 2);
        assert!(required
            .iter()
            .any(|value| value.as_str() == Some("contents")));
        assert_eq!(
            schema["properties"]["path"]["type"].as_str(),
            Some("string")
        );
        assert_eq!(schema["additionalProperties"].as_bool(), Some(false));
    }

    #[test]
    fn anthropic_tool_payload_uses_definition_schema() {
        let definition = builtin_tool_definition(ToolKind::Bash);
        let payload = definition.as_anthropic_tool();
        assert_eq!(payload["name"].as_str(), Some("bash"));
        assert_eq!(
            payload["input_schema"]["properties"]["command"]["type"].as_str(),
            Some("string")
        );
    }

    #[test]
    fn serde_tool_input_uses_internal_tool_tag() {
        let input: ToolInput = serde_json::from_value(serde_json::json!({
            "tool": "write_file",
            "path": "notes/todo.txt",
            "contents": "ship it",
        }))
        .unwrap();
        assert_eq!(
            input,
            ToolInput::WriteFile {
                path: "notes/todo.txt".into(),
                contents: "ship it".to_string(),
            }
        );
    }

    #[test]
    fn tool_input_reports_kind() {
        assert_eq!(
            ToolInput::Bash {
                command: "printf hi".to_string()
            }
            .kind(),
            ToolKind::Bash
        );
    }
}
