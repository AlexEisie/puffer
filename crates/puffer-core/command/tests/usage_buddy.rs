use super::*;

#[test]
fn usage_command_reports_runtime_and_resource_counts() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut state = AppState::new(
        PufferConfig::default(),
        tempdir.path().to_path_buf(),
        session,
    );
    state.current_provider = Some("anthropic".to_string());
    state.current_model = Some("anthropic/claude-sonnet-4-5".to_string());
    let mut providers = ProviderRegistry::new();
    providers.register(ProviderDescriptor {
        id: "anthropic".to_string(),
        display_name: "Anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        default_api: "anthropic-messages".to_string(),
        auth_modes: Vec::new(),
        headers: Default::default(),
        discovery: Some(puffer_provider_registry::ModelDiscoveryConfig {
            path: "/v1/models".to_string(),
            response: puffer_provider_registry::ModelDiscoveryFormat::AnthropicModels,
            api: "anthropic-messages".to_string(),
            context_window: 200_000,
            max_output_tokens: 8_192,
            supports_reasoning: true,
            items_field: "data".to_string(),
            id_field: "id".to_string(),
            display_name_field: Some("display_name".to_string()),
            headers: Default::default(),
        }),
        models: vec![puffer_provider_registry::ModelDescriptor {
            id: "claude-sonnet-4-5".to_string(),
            display_name: "Claude Sonnet 4.5".to_string(),
            provider: "anthropic".to_string(),
            api: "anthropic-messages".to_string(),
            context_window: 200_000,
            max_output_tokens: 8_192,
            supports_reasoning: true,
        }],
    });
    let mut auth_store = AuthStore::default();
    auth_store.set_api_key("anthropic", "sk-ant");
    let resources = LoadedResources {
        prompts: vec![LoadedItem {
            value: puffer_resources::PromptTemplate {
                id: "review".to_string(),
                description: "review".to_string(),
                template: "review".to_string(),
                variables: Vec::new(),
                provider_override: None,
                model_override: None,
                mode: None,
                chained_from: Vec::new(),
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("prompts/review.yaml"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        tools: vec![LoadedItem {
            value: puffer_resources::ToolSpec {
                id: "bash".to_string(),
                name: "bash".to_string(),
                description: "Run bash".to_string(),
                handler: "bash".to_string(),
                handler_args: Vec::new(),
                approval_policy: Some("ask".to_string()),
                sandbox_policy: Some("workspace-write".to_string()),
                shared_lib: None,
                enabled_if: None,
                input_schema: None,
                metadata: Default::default(),
                display: Default::default(),
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("tools/bash.yaml"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        hooks: vec![LoadedItem {
            value: puffer_resources::HookSpec {
                id: "tool-end".to_string(),
                event: "tool_end".to_string(),
                command: "echo done".to_string(),
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("hooks/tool_end.yaml"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        skills: vec![LoadedItem {
            value: puffer_resources::SkillSpec {
                name: "reviewer".to_string(),
                description: "review".to_string(),
                content: "review".to_string(),
                disable_model_invocation: false,
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("skills/reviewer/SKILL.md"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        plugins: vec![LoadedItem {
            value: puffer_resources::PluginSpec {
                id: "core".to_string(),
                display_name: "Core".to_string(),
                description: "core".to_string(),
                commands: Vec::new(),
                skills: Vec::new(),
                mcp_servers: Vec::new(),
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("plugins/core.yaml"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        ..LoadedResources::default()
    };

    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut providers,
        &auth_store,
        &session_store,
        "/usage",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Usage summary:")
            && text.contains("authed_providers=1")
            && text.contains("providers_with_discovery=1")
            && text.contains("prompts=1")
            && text.contains("tools=1")
            && text.contains("hooks=1")
            && text.contains("active_model=anthropic/claude-sonnet-4-5")
    ));
}

#[test]
fn buddy_command_uses_loaded_mascot_intro() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let session = session_store
        .create_session(tempdir.path().to_path_buf())
        .unwrap();
    let mut config = PufferConfig::default();
    config.mascot.id = "clawd".to_string();
    config.mascot.display_name = "Clawd".to_string();
    let mut state = AppState::new(config, tempdir.path().to_path_buf(), session);
    let resources = LoadedResources {
        mascots: vec![LoadedItem {
            value: puffer_resources::MascotSpec {
                id: "clawd".to_string(),
                display_name: "Clawd".to_string(),
                introduction: "A sharp-eyed dockside reviewer.".to_string(),
            },
            source_info: puffer_resources::SourceInfo {
                path: PathBuf::from("mascots/clawd.yaml"),
                kind: puffer_resources::SourceKind::Builtin,
            },
        }],
        ..LoadedResources::default()
    };

    dispatch_command(
        &mut state,
        &supported_commands(),
        &resources,
        &mut ProviderRegistry::new(),
        &AuthStore::default(),
        &session_store,
        "/buddy",
    )
    .unwrap();

    assert!(matches!(
        state.transcript.last(),
        Some(RenderedMessage {
            role: MessageRole::System,
            text,
        }) if text.contains("Clawd is on duty.")
            && text.contains("mascot_id=clawd")
            && text.contains("A sharp-eyed dockside reviewer.")
    ));
}
