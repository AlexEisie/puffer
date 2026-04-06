use crate::{
    builtin_tool_definition_by_handler, builtin_tool_kind, execute_builtin_tool,
    parse_builtin_input, ToolDefinition, ToolExecutionResult, ToolInput, ToolKind, ToolPolicyHints,
};
use anyhow::{anyhow, Result};
use puffer_resources::{LoadedResources, ToolSpec};
use std::collections::BTreeMap;
use std::path::Path;

/// One registered tool with its declarative metadata and runtime kind.
#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub spec: ToolDefinition,
    pub kind: ToolKind,
}

impl RegisteredTool {
    /// Returns the model-facing definition for the registered tool.
    pub fn definition(&self) -> &ToolDefinition {
        &self.spec
    }
}

/// Registry of executable built-in tools derived from loaded resources.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
}

impl ToolRegistry {
    /// Builds a tool registry from declarative tool definitions.
    pub fn from_definitions(definitions: impl IntoIterator<Item = ToolDefinition>) -> Self {
        let mut registry = Self::default();
        for definition in definitions {
            let _ = registry.register(definition);
        }
        registry
    }

    /// Builds a tool registry from loaded declarative resources.
    pub fn from_resources(resources: &LoadedResources) -> Self {
        Self::from_definitions(
            resources
                .tools
                .iter()
                .filter_map(|item| definition_from_spec(&item.value)),
        )
    }

    /// Registers one declarative tool definition when it maps to a built-in handler.
    pub fn register(&mut self, definition: ToolDefinition) -> Result<()> {
        let kind = builtin_tool_kind(&definition.handler)
            .ok_or_else(|| anyhow!("unsupported built-in handler {}", definition.handler))?;
        self.tools.insert(
            definition.id.clone(),
            RegisteredTool {
                spec: definition,
                kind,
            },
        );
        Ok(())
    }

    /// Returns the number of executable tools in the registry.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true when the registry has no executable tools.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Returns all registered tools in stable id order.
    pub fn tools(&self) -> impl Iterator<Item = &RegisteredTool> {
        self.tools.values()
    }

    /// Returns all model-facing tool definitions in stable id order.
    pub fn definitions(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.tools.values().map(RegisteredTool::definition)
    }

    /// Looks up a registered tool by id.
    pub fn tool(&self, tool_id: &str) -> Option<&RegisteredTool> {
        self.tools.get(tool_id)
    }

    /// Looks up a model-facing tool definition by id.
    pub fn definition(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.tool(tool_id).map(RegisteredTool::definition)
    }

    /// Executes a registered tool by id with typed input.
    pub fn execute(
        &self,
        tool_id: &str,
        cwd: &Path,
        input: ToolInput,
    ) -> Result<ToolExecutionResult> {
        let tool = self
            .tool(tool_id)
            .ok_or_else(|| anyhow!("unknown tool {tool_id}"))?;
        execute_builtin_tool(tool_id, tool.kind, cwd, input)
    }

    /// Executes a registered tool by id using a raw JSON input payload.
    pub fn execute_json(
        &self,
        tool_id: &str,
        cwd: &Path,
        input: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        let tool = self
            .tool(tool_id)
            .ok_or_else(|| anyhow!("unknown tool {tool_id}"))?;
        let typed = parse_builtin_input(tool.kind, input)?;
        execute_builtin_tool(tool_id, tool.kind, cwd, typed)
    }
}

fn definition_from_spec(spec: &ToolSpec) -> Option<ToolDefinition> {
    let mut definition = builtin_tool_definition_by_handler(&spec.handler)?;
    definition.id = spec.id.clone();
    definition.name = spec.name.clone();
    definition.description = spec.description.clone();
    definition.handler = spec.handler.clone();
    definition.policy = ToolPolicyHints {
        approval_policy: spec.approval_policy.clone(),
        sandbox_policy: spec.sandbox_policy.clone(),
    };
    Some(definition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_resources::{LoadedItem, SourceInfo, SourceKind};
    use std::path::PathBuf;

    fn bash_tool_spec() -> ToolSpec {
        ToolSpec {
            id: "bash".to_string(),
            name: "bash".to_string(),
            description: "Run shell".to_string(),
            handler: "bash".to_string(),
            approval_policy: Some("on-request".to_string()),
            sandbox_policy: Some("workspace-write".to_string()),
        }
    }

    #[test]
    fn registry_builds_from_resources() {
        let resources = LoadedResources {
            tools: vec![LoadedItem {
                value: bash_tool_spec(),
                source_info: SourceInfo {
                    path: PathBuf::from("bash.yaml"),
                    kind: SourceKind::Builtin,
                },
            }],
            ..LoadedResources::default()
        };
        let registry = ToolRegistry::from_resources(&resources);
        assert!(registry.tool("bash").is_some());
    }

    #[test]
    fn registry_builds_from_definitions() {
        let registry = ToolRegistry::from_definitions([ToolKind::ReadFile.definition()]);
        assert_eq!(registry.len(), 1);
        assert!(registry.tool("read_file").is_some());
    }

    #[test]
    fn register_rejects_unknown_handlers() {
        let mut registry = ToolRegistry::default();
        let error = registry
            .register(ToolDefinition {
                id: "custom".to_string(),
                name: "custom".to_string(),
                description: "Custom".to_string(),
                handler: "custom_handler".to_string(),
                kind: ToolKind::Bash,
                input_schema: ToolKind::Bash.definition().input_schema,
                metadata: ToolKind::Bash.definition().metadata,
                policy: ToolPolicyHints::default(),
            })
            .unwrap_err();
        assert!(error.to_string().contains("unsupported built-in handler"));
    }

    #[test]
    fn registry_ignores_unknown_handlers() {
        let resources = LoadedResources {
            tools: vec![LoadedItem {
                value: ToolSpec {
                    id: "custom".to_string(),
                    name: "custom".to_string(),
                    description: "Custom".to_string(),
                    handler: "custom_handler".to_string(),
                    approval_policy: None,
                    sandbox_policy: None,
                },
                source_info: SourceInfo {
                    path: PathBuf::from("custom.yaml"),
                    kind: SourceKind::Builtin,
                },
            }],
            ..LoadedResources::default()
        };
        let registry = ToolRegistry::from_resources(&resources);
        assert!(registry.tool("custom").is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn resource_registry_carries_model_metadata_and_policy_hints() {
        let resources = LoadedResources {
            tools: vec![LoadedItem {
                value: bash_tool_spec(),
                source_info: SourceInfo {
                    path: PathBuf::from("bash.yaml"),
                    kind: SourceKind::Builtin,
                },
            }],
            ..LoadedResources::default()
        };
        let registry = ToolRegistry::from_resources(&resources);
        let definition = registry.definition("bash").unwrap();
        assert_eq!(definition.description, "Run shell");
        assert_eq!(
            definition.policy.approval_policy.as_deref(),
            Some("on-request")
        );
        assert_eq!(
            definition.input_schema.properties["command"].value_type,
            crate::ToolSchemaType::String
        );
        assert!(definition.metadata.may_spawn_processes);
    }

    #[test]
    fn execute_json_parses_input_for_registered_tool() {
        let resources = LoadedResources {
            tools: vec![LoadedItem {
                value: bash_tool_spec(),
                source_info: SourceInfo {
                    path: PathBuf::from("bash.yaml"),
                    kind: SourceKind::Builtin,
                },
            }],
            ..LoadedResources::default()
        };
        let registry = ToolRegistry::from_resources(&resources);
        let temp = tempfile::tempdir().unwrap();
        let result = registry
            .execute_json(
                "bash",
                temp.path(),
                serde_json::json!({ "command": "printf hi" }),
            )
            .unwrap();
        assert_eq!(result.output.stdout, "hi");
    }
}
