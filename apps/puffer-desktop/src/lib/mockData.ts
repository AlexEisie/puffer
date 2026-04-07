import type {
  AuthProviderStatus,
  DesktopPreferences,
  DiffSnapshot,
  FolderGroup,
  ProviderSummary,
  RepoActionResult,
  RepoStatus,
  SessionDetail,
  SessionListItem,
  SettingsSnapshot,
  TimelineItem
} from "./types";

const now = Date.now();

const sessionA: SessionListItem = {
  id: "session-a",
  displayName: "Claude parity sweep",
  title: "Claude parity sweep",
  cwd: "/home/c/puffer",
  folderPath: "/home/c/puffer",
  createdAtMs: now - 6_800_000,
  updatedAtMs: now - 42_000,
  eventCount: 52,
  slug: "claude-parity-sweep",
  tags: ["parity", "tools"],
  note: "Tight parity work against Claude Code",
  parentSessionId: null
};

const sessionB: SessionListItem = {
  id: "session-b",
  displayName: "Desktop shell",
  title: "Desktop shell",
  cwd: "/home/c/puffer/.worktree/tauri-desktop-ui",
  folderPath: "/home/c/puffer",
  createdAtMs: now - 12_500_000,
  updatedAtMs: now - 130_000,
  eventCount: 31,
  slug: "desktop-shell",
  tags: ["desktop", "ui"],
  note: "Parallel Tauri 2 app exploration",
  parentSessionId: null
};

const sessionC: SessionListItem = {
  id: "session-c",
  displayName: "Python LSP validation",
  title: "Python LSP validation",
  cwd: "/home/c/sample-python",
  folderPath: "/home/c/sample-python",
  createdAtMs: now - 20_000_000,
  updatedAtMs: now - 600_000,
  eventCount: 18,
  slug: "python-lsp-validation",
  tags: ["lsp", "python"],
  note: "Real pyright end-to-end check",
  parentSessionId: null
};

export const mockFolders: FolderGroup[] = [
  {
    id: "/home/c/puffer",
    label: "puffer",
    path: "/home/c/puffer",
    sessionCount: 2,
    sessions: [sessionA, sessionB]
  },
  {
    id: "/home/c/sample-python",
    label: "sample-python",
    path: "/home/c/sample-python",
    sessionCount: 1,
    sessions: [sessionC]
  }
];

export const mockRepoStatus: RepoStatus = {
  sessionId: sessionB.id,
  cwd: sessionB.cwd,
  isGitRepo: true,
  repoRoot: "/home/c/puffer",
  branch: "feature/tauri-desktop-ui",
  headSha: "3ca43a4",
  isClean: false,
  hasUncommittedChanges: true,
  statusLines: [" M apps/puffer-desktop/src/App.svelte", "?? apps/puffer-desktop/src-tauri/"],
  ghAvailable: true,
  ghAuthenticated: true,
  canCreatePr: true,
  canMergePr: true,
  createPrReason: null,
  mergePrReason: null,
  pullRequest: {
    number: 42,
    title: "Add Tauri desktop UI",
    url: "https://github.com/berabuddies/puffer/pull/42",
    state: "OPEN",
    mergeStateStatus: "CLEAN",
    isDraft: false,
    baseRefName: "master",
    headRefName: "feature/tauri-desktop-ui"
  },
  warnings: []
};

const latestDiff: DiffSnapshot = {
  id: "diff-latest",
  source: "session_history",
  title: "Desktop UI shell and inspector layout",
  command: "/review",
  status: "6 files changed, 214 insertions(+), 0 deletions(-)",
  unstagedDiffstat: "apps/puffer-desktop/src/*",
  stagedDiffstat: "",
  patchExcerpt: [
    "@@ -0,0 +1,62 @@",
    "+<script lang=\"ts\">",
    "+  let inspectorTab = \"latest-diff\";",
    "+  let inspectorOpen = true;",
    "+</script>",
    "+",
    "+<main class=\"shell\">",
    "+  <SessionSidebar />",
    "+  <ConversationPane />",
    "+  <InspectorPane />",
    "+</main>"
  ].join("\n")
};

const olderDiff: DiffSnapshot = {
  id: "diff-older",
  source: "session_history",
  title: "Runtime session grouping API",
  command: "/diff",
  status: "3 files changed, 121 insertions(+), 0 deletions(-)",
  unstagedDiffstat: "",
  stagedDiffstat: "apps/puffer-desktop/src-tauri/src/session_data.rs",
  patchExcerpt: [
    "@@ -0,0 +1,80 @@",
    "+#[tauri::command]",
    "+pub fn list_grouped_sessions() -> Result<Vec<FolderGroupDto>, String> {",
    "+    load_groups()",
    "+}"
  ].join("\n")
};

const timeline: TimelineItem[] = [
  {
    id: "msg-1",
    kind: "user",
    title: "User message",
    summary: "Implement a Tauri UI for this.",
    body: "Implement a Tauri UI for this. It should feel minimal and production-ready.",
    meta: ["Turn 1"]
  },
  {
    id: "msg-2",
    kind: "assistant",
    title: "Assistant response",
    summary: "Scaffolding a parallel desktop app with shared session data.",
    body: [
      "I’m scaffolding the desktop app as a parallel Tauri 2 UI.",
      "",
      "The frontend will browse folders and sessions on the left, and keep the conversation with diff inspection on the right."
    ].join("\n"),
    meta: ["Turn 1"]
  },
  {
    id: "cmd-1",
    kind: "command",
    title: "/review",
    summary: "Captured a new git diff checkpoint.",
    body: "/review",
    meta: ["slash command"]
  },
  {
    id: "tool-1",
    kind: "tool",
    title: "Tool call: Bash",
    summary: "Checked local GitHub CLI availability.",
    body: "Executed `gh --version` to determine whether one-click PR actions can be enabled.",
    meta: ["Bash", "ok"],
    toolName: "Bash",
    status: "ok",
    input: "{\"command\":\"gh --version\"}",
    output: "gh version 2.89.0",
    inputJson: { command: "gh --version" }
  },
  {
    id: "perm-1",
    kind: "permission",
    title: "Permission request",
    summary: "Config write requires approval before the turn can continue.",
    body: [
      "Tool: Config",
      "Scope: workspace",
      "Reason: workspace permission rule requires approval"
    ].join("\n"),
    meta: ["required"],
    toolName: "Config",
    status: "required",
    permissionDialog: {
      state: "required",
      reason: "workspace permission rule requires approval",
      summary: "Setting: theme",
      inputText: "{\"setting\":\"theme\",\"value\":\"light\"}",
      toolName: "Config",
      choices: ["Allow once", "Allow for session", "Deny"]
    },
    scopeLabel: "workspace",
    choices: ["Allow once", "Allow for session", "Deny"]
  },
  {
    id: "diff-1",
    kind: "diff",
    title: latestDiff.title,
    summary: latestDiff.status,
    body: latestDiff.patchExcerpt,
    meta: [latestDiff.command],
    diff: latestDiff
  },
  {
    id: "msg-3",
    kind: "assistant",
    title: "Assistant response",
    summary: "Introduced a three-pane desktop layout with PR actions.",
    body: [
      "The desktop shell now includes:",
      "- folder and session navigation",
      "- a structured conversation timeline",
      "- an inspector for latest diff, history, and tool details",
      "- one-click PR and merge actions"
    ].join("\n"),
    meta: ["Turn 2"]
  }
];

export const mockSessionDetail: SessionDetail = {
  session: sessionB,
  timeline,
  latestDiff,
  diffHistory: [latestDiff, olderDiff],
  repoStatus: mockRepoStatus
};

export function mockCreatePrResult(): RepoActionResult {
  return {
    ok: true,
    action: "create_pull_request",
    message: "Created pull request #42.",
    repoStatus: mockRepoStatus,
    pullRequest: mockRepoStatus.pullRequest
  };
}

export function mockMergePrResult(): RepoActionResult {
  return {
    ok: true,
    action: "merge_pull_request",
    message: "Pull request merged successfully.",
    repoStatus: {
      ...mockRepoStatus,
      isClean: true,
      hasUncommittedChanges: false,
      canMergePr: false,
      mergePrReason: "No active pull request exists for the current branch.",
      pullRequest: null
    },
    pullRequest: null
  };
}

export const mockDesktopPreferences: DesktopPreferences = {
  rememberSession: true,
  rememberInspectorLayout: true,
  launchInspectorOpen: true,
  defaultInspectorTab: "latest-diff",
  defaultInspectorWidth: 50,
  remoteEnabled: false,
  remoteTarget: "",
  remoteCwd: ""
};

const mockAuth: AuthProviderStatus[] = [
  {
    providerId: "anthropic",
    kind: "oauth",
    email: "developer@example.com",
    expiresAtMs: Date.now() + 86_400_000,
    scopes: ["org:create_api_key", "user:profile"],
    planType: "max",
    organizationName: "Berabuddies"
  },
  {
    providerId: "openai",
    kind: "api_key",
    email: null,
    expiresAtMs: null,
    scopes: [],
    planType: null,
    organizationName: null
  }
];

const mockProviders: ProviderSummary[] = [
  {
    id: "anthropic",
    displayName: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    defaultApi: "anthropic-messages",
    modelCount: 3,
    authModes: ["api_key", "oauth"],
    sourceKind: "resourcepack",
    sourcePath: "resources/providers/anthropic.yaml"
  },
  {
    id: "openai",
    displayName: "OpenAI",
    baseUrl: "https://api.openai.com",
    defaultApi: "openai-responses",
    modelCount: 6,
    authModes: ["api_key", "oauth"],
    sourceKind: "resourcepack",
    sourcePath: "resources/providers/openai.yaml"
  }
];

export const mockSettingsSnapshot: SettingsSnapshot = {
  workspaceRoot: "/home/c/puffer",
  workspaceConfigFile: "/home/c/puffer/.puffer/config.toml",
  userConfigFile: "/home/c/.puffer/config.toml",
  authStoreFile: "/home/c/.puffer/auth.json",
  builtinResourcesDir: "/home/c/puffer/resources",
  config: {
    appName: "Puffer Code",
    defaultProvider: "anthropic",
    defaultModel: "claude-sonnet-4-5",
    openaiBaseUrl: null,
    theme: "puffer",
    mascotId: "clawd",
    mascotDisplayName: "Clawd",
    mascotEnabled: true,
    uiNoAltScreen: false,
    uiTmuxGoldenMode: false
  },
  resources: {
    providers: 2,
    tools: 38,
    agents: 4,
    prompts: 7,
    hooks: 0,
    skills: 1,
    mascots: 1,
    plugins: 1,
    mcpServers: 2,
    ides: 1
  },
  sessions: {
    totalSessions: 3,
    folderGroups: 2
  },
  auth: mockAuth,
  providers: mockProviders
};
