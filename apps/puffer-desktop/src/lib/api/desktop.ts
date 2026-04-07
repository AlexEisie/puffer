import { invoke } from "@tauri-apps/api/core";
import type {
  AuthProviderStatus,
  DiffSnapshot,
  FolderGroup,
  ProviderSummary,
  PullRequest,
  RemoteConnection,
  RemoteOperation,
  RepoActionResult,
  RepoStatus,
  SessionDetail,
  SessionListItem,
  SettingsSnapshot,
  TimelineItem
} from "../types";
import {
  mockCreatePrResult,
  mockFolders,
  mockMergePrResult,
  mockRepoStatus,
  mockSessionDetail,
  mockSettingsSnapshot
} from "../mockData";

type BackendFolderGroup = {
  folderId: string;
  folderLabel: string;
  folderPath: string;
  sessionCount: number;
  sessions: BackendSessionListItem[];
};

type BackendSessionListItem = {
  sessionId: string;
  displayName: string | null;
  title: string;
  cwd: string;
  folderPath: string;
  updatedAtMs: number;
  createdAtMs: number;
  eventCount: number;
  slug: string | null;
  tags: string[];
  note: string | null;
  parentSessionId: string | null;
};

type BackendDiff = {
  id: string;
  source: string;
  commandLabel: string;
  statusText: string;
  unstagedDiffstat: string;
  stagedDiffstat: string;
  patchExcerpt: string;
};

type BackendPullRequest = {
  number: number;
  title: string;
  url: string;
  state: string;
  isDraft: boolean;
  mergeStateStatus: string | null;
  headRefName: string | null;
  baseRefName: string | null;
};

type BackendRepoStatus = {
  sessionId: string;
  cwd: string;
  repoRoot: string | null;
  branch: string | null;
  headSha: string | null;
  isClean: boolean;
  statusLines: string[];
  hasGh: boolean;
  ghAuthenticated: boolean;
  canCreatePullRequest: boolean;
  canMergePullRequest: boolean;
  createPullRequestReason: string | null;
  mergePullRequestReason: string | null;
  openPullRequest: BackendPullRequest | null;
  warnings: string[];
};

type BackendRepoActionResult = {
  ok: boolean;
  action: string;
  message: string;
  repoStatus: BackendRepoStatus;
  pullRequest: BackendPullRequest | null;
};

type BackendTimelineItem =
  | { kind: "user_message"; id: string; text: string }
  | { kind: "assistant_message"; id: string; text: string }
  | { kind: "system_message"; id: string; text: string }
  | { kind: "command"; id: string; commandName: string; commandArgs: string }
  | {
      kind: "tool_call";
      id: string;
      toolId: string;
      status: string;
      summary: string | null;
      inputText: string;
      inputJson: Record<string, unknown> | null;
      outputText: string;
    }
  | {
      kind: "permission_dialog";
      id: string;
      toolId: string;
      state: string;
      summary: string | null;
      reason: string;
      inputText: string | null;
    }
  | { kind: "diff_snapshot"; id: string; snapshot: BackendDiff };

type BackendSessionDetail = BackendSessionListItem & {
  timeline: BackendTimelineItem[];
  latestDiff: BackendDiff | null;
  diffHistory: BackendDiff[];
  repoStatus: BackendRepoStatus;
};

function remoteArgs(remote?: RemoteConnection): Record<string, unknown> {
  if (!remote || !remote.enabled || !remote.target.trim()) {
    return {};
  }
  return {
    remoteTarget: remote.target,
    remoteCwd: remote.cwd || null,
    remotePassword: remote.password || null
  };
}

type BackendSettingsConfig = SettingsSnapshot["config"];
type BackendResourceCounts = SettingsSnapshot["resources"];
type BackendSettingsSessionSummary = SettingsSnapshot["sessions"];
type BackendAuthProviderStatus = AuthProviderStatus;
type BackendProviderSummary = ProviderSummary;

type BackendSettingsSnapshot = {
  workspaceRoot: string;
  workspaceConfigFile: string;
  userConfigFile: string;
  authStoreFile: string;
  builtinResourcesDir: string;
  config: BackendSettingsConfig;
  resources: BackendResourceCounts;
  sessions: BackendSettingsSessionSummary;
  auth: BackendAuthProviderStatus[];
  providers: BackendProviderSummary[];
};

type BackendRemoteOperation = RemoteOperation;

function canInvokeTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function preview(text: string, maxLength = 160): string {
  return text.length > maxLength ? `${text.slice(0, maxLength).trimEnd()}...` : text;
}

function normalizePullRequest(value: BackendPullRequest | null): PullRequest | null {
  if (!value) {
    return null;
  }
  return {
    number: value.number,
    title: value.title,
    url: value.url,
    state: value.state,
    isDraft: value.isDraft,
    mergeStateStatus: value.mergeStateStatus,
    headRefName: value.headRefName,
    baseRefName: value.baseRefName
  };
}

function normalizeRepoStatus(value: BackendRepoStatus): RepoStatus {
  return {
    sessionId: value.sessionId,
    cwd: value.cwd,
    isGitRepo: value.repoRoot !== null,
    repoRoot: value.repoRoot,
    branch: value.branch,
    headSha: value.headSha,
    isClean: value.isClean,
    hasUncommittedChanges: !value.isClean,
    statusLines: value.statusLines,
    ghAvailable: value.hasGh,
    ghAuthenticated: value.ghAuthenticated,
    canCreatePr: value.canCreatePullRequest,
    canMergePr: value.canMergePullRequest,
    createPrReason: value.createPullRequestReason,
    mergePrReason: value.mergePullRequestReason,
    pullRequest: normalizePullRequest(value.openPullRequest),
    warnings: value.warnings
  };
}

function normalizeDiff(value: BackendDiff): DiffSnapshot {
  return {
    id: value.id,
    source: value.source,
    title: value.commandLabel,
    command: value.commandLabel,
    status: value.statusText,
    unstagedDiffstat: value.unstagedDiffstat,
    stagedDiffstat: value.stagedDiffstat,
    patchExcerpt: value.patchExcerpt
  };
}

function normalizeSessionListItem(value: BackendSessionListItem): SessionListItem {
  return {
    id: value.sessionId,
    displayName: value.displayName,
    title: value.title,
    cwd: value.cwd,
    folderPath: value.folderPath,
    updatedAtMs: value.updatedAtMs,
    createdAtMs: value.createdAtMs,
    eventCount: value.eventCount,
    slug: value.slug,
    tags: value.tags,
    note: value.note,
    parentSessionId: value.parentSessionId
  };
}

function normalizeTimelineItem(value: BackendTimelineItem): TimelineItem {
  switch (value.kind) {
    case "user_message":
      return {
        id: value.id,
        kind: "user",
        title: "User message",
        summary: preview(value.text),
        body: value.text,
        meta: []
      };
    case "assistant_message":
      return {
        id: value.id,
        kind: "assistant",
        title: "Assistant response",
        summary: preview(value.text),
        body: value.text,
        meta: []
      };
    case "system_message":
      return {
        id: value.id,
        kind: "system",
        title: "System message",
        summary: preview(value.text),
        body: value.text,
        meta: []
      };
    case "command":
      return {
        id: value.id,
        kind: "command",
        title: `/${value.commandName}`,
        summary: preview(value.commandArgs || `/${value.commandName}`),
        body: [value.commandName, value.commandArgs].filter(Boolean).join(" "),
        meta: ["slash command"]
      };
    case "tool_call":
      return {
        id: value.id,
        kind: "tool",
        title: `Tool call: ${value.toolId}`,
        summary: value.summary ?? preview(value.outputText || value.inputText),
        body: value.outputText || "Tool call completed without textual output.",
        meta: [value.toolId, value.status],
        toolName: value.toolId,
        status: value.status,
        input: value.inputText,
        output: value.outputText,
        inputJson: value.inputJson
      };
    case "permission_dialog":
      return {
        id: value.id,
        kind: "permission",
        title: "Permission request",
        summary: value.summary ?? `${value.toolId} requires approval`,
        body: `Tool: ${value.toolId}\nReason: ${value.reason}`,
        meta: [value.state],
        toolName: value.toolId,
        status: value.state,
        permissionDialog: {
          state: value.state,
          reason: value.reason,
          summary: value.summary,
          inputText: value.inputText,
          toolName: value.toolId,
          choices: ["Allow once", "Allow for session", "Deny"]
        },
        scopeLabel: "workspace",
        choices: ["Allow once", "Allow for session", "Deny"]
      };
    case "diff_snapshot": {
      const diff = normalizeDiff(value.snapshot);
      return {
        id: value.id,
        kind: "diff",
        title: diff.title,
        summary: diff.status,
        body: diff.patchExcerpt,
        meta: [diff.command],
        diff
      };
    }
  }
}

function normalizeSessionDetail(value: BackendSessionDetail): SessionDetail {
  const session = normalizeSessionListItem(value);
  return {
    session,
    timeline: value.timeline.map(normalizeTimelineItem),
    latestDiff: value.latestDiff ? normalizeDiff(value.latestDiff) : null,
    diffHistory: value.diffHistory.map(normalizeDiff),
    repoStatus: normalizeRepoStatus(value.repoStatus)
  };
}

export async function listGroupedSessions(remote?: RemoteConnection): Promise<FolderGroup[]> {
  if (!canInvokeTauri()) {
    return mockFolders;
  }
  const response = await invoke<BackendFolderGroup[]>("list_grouped_sessions", remoteArgs(remote));
  return response.map((group) => ({
    id: group.folderId,
    label: group.folderLabel,
    path: group.folderPath,
    sessionCount: group.sessionCount,
    sessions: group.sessions.map(normalizeSessionListItem)
  }));
}

export async function loadSessionDetail(
  sessionId: string,
  remote?: RemoteConnection
): Promise<SessionDetail> {
  if (!canInvokeTauri()) {
    return mockSessionDetail;
  }
  const response = await invoke<BackendSessionDetail>("load_session_detail", {
    sessionId,
    ...remoteArgs(remote)
  });
  return normalizeSessionDetail(response);
}

export async function refreshRepoStatus(
  sessionId: string,
  remote?: RemoteConnection
): Promise<RepoStatus> {
  if (!canInvokeTauri()) {
    return mockRepoStatus;
  }
  const response = await invoke<BackendRepoStatus>("refresh_repo_status", {
    sessionId,
    ...remoteArgs(remote)
  });
  return normalizeRepoStatus(response);
}

export async function createPullRequest(
  sessionId: string,
  title?: string,
  body?: string,
  remote?: RemoteConnection
): Promise<RepoActionResult> {
  if (!canInvokeTauri()) {
    return mockCreatePrResult();
  }
  const response = await invoke<BackendRepoActionResult>("create_pull_request", {
    sessionId,
    title: title ?? null,
    body: body ?? null,
    ...remoteArgs(remote)
  });
  return {
    ok: response.ok,
    action: response.action,
    message: response.message,
    repoStatus: normalizeRepoStatus(response.repoStatus),
    pullRequest: normalizePullRequest(response.pullRequest)
  };
}

export async function mergePullRequest(
  sessionId: string,
  pullRequestNumber?: number,
  mergeMethod?: string,
  remote?: RemoteConnection
): Promise<RepoActionResult> {
  if (!canInvokeTauri()) {
    return mockMergePrResult();
  }
  const response = await invoke<BackendRepoActionResult>("merge_pull_request", {
    sessionId,
    pullRequestNumber: pullRequestNumber ?? null,
    mergeMethod: mergeMethod ?? null,
    ...remoteArgs(remote)
  });
  return {
    ok: response.ok,
    action: response.action,
    message: response.message,
    repoStatus: normalizeRepoStatus(response.repoStatus),
    pullRequest: normalizePullRequest(response.pullRequest)
  };
}

export async function loadSettingsSnapshot(remote?: RemoteConnection): Promise<SettingsSnapshot> {
  if (!canInvokeTauri()) {
    return mockSettingsSnapshot;
  }
  return invoke<BackendSettingsSnapshot>("load_settings_snapshot", remoteArgs(remote));
}

export async function loginWithOauth(
  providerId: string,
  remote?: RemoteConnection
): Promise<SettingsSnapshot> {
  if (!canInvokeTauri()) {
    return mockSettingsSnapshot;
  }
  return invoke<BackendSettingsSnapshot>("login_with_oauth", {
    providerId,
    ...remoteArgs(remote)
  });
}

export async function loginWithApiKey(
  providerId: string,
  apiKey: string,
  remote?: RemoteConnection
): Promise<SettingsSnapshot> {
  if (!canInvokeTauri()) {
    return mockSettingsSnapshot;
  }
  return invoke<BackendSettingsSnapshot>("login_with_api_key", {
    providerId,
    apiKey,
    ...remoteArgs(remote)
  });
}

export async function logoutProvider(
  providerId: string,
  remote?: RemoteConnection
): Promise<SettingsSnapshot> {
  if (!canInvokeTauri()) {
    return mockSettingsSnapshot;
  }
  return invoke<BackendSettingsSnapshot>("logout_provider", {
    providerId,
    ...remoteArgs(remote)
  });
}

export async function runRemoteBash(
  remote: RemoteConnection,
  command: string
): Promise<RemoteOperation> {
  if (!canInvokeTauri()) {
    return { success: true, stdout: "mock remote bash\n", stderr: "" };
  }
  return invoke<BackendRemoteOperation>("run_remote_bash", {
    remoteTarget: remote.target,
    remoteCwd: remote.cwd || null,
    remotePassword: remote.password || null,
    command
  });
}

export async function readRemoteFile(
  remote: RemoteConnection,
  path: string
): Promise<RemoteOperation> {
  if (!canInvokeTauri()) {
    return { success: true, stdout: "mock remote file\n", stderr: "" };
  }
  return invoke<BackendRemoteOperation>("read_remote_file", {
    remoteTarget: remote.target,
    remoteCwd: remote.cwd || null,
    remotePassword: remote.password || null,
    path
  });
}

export async function writeRemoteFile(
  remote: RemoteConnection,
  path: string,
  contents: string
): Promise<RemoteOperation> {
  if (!canInvokeTauri()) {
    return { success: true, stdout: "", stderr: "" };
  }
  return invoke<BackendRemoteOperation>("write_remote_file", {
    remoteTarget: remote.target,
    remoteCwd: remote.cwd || null,
    remotePassword: remote.password || null,
    path,
    contentsBase64: btoa(unescape(encodeURIComponent(contents)))
  });
}
