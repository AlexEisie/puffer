<script lang="ts">
  import { onMount } from "svelte";
  import HeaderBar from "./lib/components/HeaderBar.svelte";
  import SessionSidebar from "./lib/components/SessionSidebar.svelte";
  import ConversationPane from "./lib/components/ConversationPane.svelte";
  import InspectorPane from "./lib/components/InspectorPane.svelte";
  import OverviewStrip from "./lib/components/OverviewStrip.svelte";
  import SettingsView from "./lib/components/SettingsView.svelte";
  import LoginView from "./lib/components/LoginView.svelte";
  import {
    createPullRequest,
    loginWithApiKey,
    loginWithOauth,
    listGroupedSessions,
    loadSettingsSnapshot,
    loadSessionDetail,
    mergePullRequest,
    logoutProvider,
    readRemoteFile,
    refreshRepoStatus,
    runRemoteBash,
    writeRemoteFile
  } from "./lib/api/desktop";
  import type {
    AppView,
    DesktopPreferences,
    FolderGroup,
    InspectorTab,
    RemoteConnection,
    RemoteOperation,
    SessionDetail,
    SessionListItem,
    SettingsSnapshot,
    TimelineItem
  } from "./lib/types";

  let groups: FolderGroup[] = [];
  let selectedSession: SessionListItem | null = null;
  let sessionDetail: SessionDetail | null = null;
  let settingsSnapshot: SettingsSnapshot | null = null;
  let selectedItem: TimelineItem | null = null;
  let view: AppView = "workspace";
  let inspectorOpen = true;
  let inspectorTab: InspectorTab = "latest-diff";
  let inspectorWidth = 50;
  let statusMessage = "Desktop workspace ready.";
  let groupsLoading = true;
  let sessionLoading = false;
  let settingsLoading = false;
  let actionBusy = false;
  let authBusyProviderId: string | null = null;
  let authError: string | null = null;
  let remoteOperation: RemoteOperation | null = null;
  let remoteBusy = false;
  let preferredSessionId: string | null = null;
  let remotePassword = "";
  let contentElement: HTMLDivElement | null = null;
  let isResizingInspector = false;

  const defaultDesktopPreferences: DesktopPreferences = {
    rememberSession: true,
    rememberInspectorLayout: true,
    launchInspectorOpen: true,
    defaultInspectorTab: "latest-diff",
    defaultInspectorWidth: 50,
    remoteEnabled: false,
    remoteTarget: "",
    remoteCwd: ""
  };
  let desktopPreferences: DesktopPreferences = { ...defaultDesktopPreferences };

  const storageKeys = {
    sessionId: "puffer-desktop:selected-session",
    inspectorOpen: "puffer-desktop:inspector-open",
    inspectorTab: "puffer-desktop:inspector-tab",
    inspectorWidth: "puffer-desktop:inspector-width",
    prefs: "puffer-desktop:preferences"
  } as const;

  $: timeline = sessionDetail?.timeline ?? [];
  $: pendingPermissionCount = timeline.filter((item) => item.kind === "permission").length;
  $: toolCount = timeline.filter((item) => item.kind === "tool").length;
  $: diffCount = timeline.filter((item) => item.kind === "diff").length;
  $: selectedLabel = selectedItem ? `${selectedItem.kind}: ${selectedItem.title}` : "No focused item";
  $: remoteConnection = {
    enabled:
      desktopPreferences.remoteEnabled && desktopPreferences.remoteTarget.trim().length > 0,
    target: desktopPreferences.remoteTarget.trim(),
    cwd: desktopPreferences.remoteCwd.trim(),
    password: remotePassword
  } satisfies RemoteConnection;

  function syncInspectorForItem(item: TimelineItem | null) {
    selectedItem = item;
    if (!item) {
      return;
    }
    inspectorTab =
      item.kind === "diff"
        ? "latest-diff"
        : item.kind === "tool" || item.kind === "permission"
          ? "tool-details"
          : "history";
  }

  function chooseDefaultItem(items: TimelineItem[]): TimelineItem | null {
    return (
      items.find((item) => item.kind === "permission") ??
      items.find((item) => item.kind === "tool") ??
      items.find((item) => item.kind === "diff") ??
      items[0] ??
      null
    );
  }

  function buildPrDefaults(session: SessionListItem) {
    return {
      title: session.displayName ?? session.title,
      body: [
        `Generated from session: ${session.title}`,
        session.note ? `Context: ${session.note}` : null
      ]
        .filter(Boolean)
        .join("\n")
    };
  }

  function clampInspectorWidth(value: number): number {
    return Math.min(68, Math.max(32, value));
  }

  function updateInspectorWidth(clientX: number) {
    if (!contentElement) {
      return;
    }
    const rect = contentElement.getBoundingClientRect();
    if (rect.width <= 0) {
      return;
    }
    inspectorWidth = clampInspectorWidth(((rect.right - clientX) / rect.width) * 100);
  }

  function beginInspectorResize(event: PointerEvent) {
    if (!inspectorOpen) {
      return;
    }
    isResizingInspector = true;
    updateInspectorWidth(event.clientX);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    event.preventDefault();
  }

  function nudgeInspector(event: KeyboardEvent) {
    if (!inspectorOpen) {
      return;
    }
    if (event.key === "ArrowLeft") {
      inspectorWidth = clampInspectorWidth(inspectorWidth + 2);
      event.preventDefault();
    } else if (event.key === "ArrowRight") {
      inspectorWidth = clampInspectorWidth(inspectorWidth - 2);
      event.preventDefault();
    } else if (event.key === "Enter") {
      inspectorWidth = 50;
      event.preventDefault();
    }
  }

  function restoreDesktopState() {
    if (typeof window === "undefined") {
      return;
    }
    const rawPrefs = window.localStorage.getItem(storageKeys.prefs);
    if (rawPrefs) {
      try {
        desktopPreferences = {
          ...defaultDesktopPreferences,
          ...JSON.parse(rawPrefs)
        };
      } catch {
        desktopPreferences = { ...defaultDesktopPreferences };
      }
    }
    preferredSessionId = desktopPreferences.rememberSession
      ? window.localStorage.getItem(storageKeys.sessionId)
      : null;
    const storedInspectorOpen = window.localStorage.getItem(storageKeys.inspectorOpen);
    const storedInspectorTab = window.localStorage.getItem(storageKeys.inspectorTab);
    if (desktopPreferences.rememberInspectorLayout) {
      if (storedInspectorOpen === "false") {
        inspectorOpen = false;
      }
      if (
        storedInspectorTab === "latest-diff" ||
        storedInspectorTab === "history" ||
        storedInspectorTab === "tool-details"
      ) {
        inspectorTab = storedInspectorTab;
      }
      const storedInspectorWidth = Number(window.localStorage.getItem(storageKeys.inspectorWidth));
      if (Number.isFinite(storedInspectorWidth) && storedInspectorWidth > 0) {
        inspectorWidth = clampInspectorWidth(storedInspectorWidth);
      }
    } else {
      inspectorOpen = desktopPreferences.launchInspectorOpen;
      inspectorTab = desktopPreferences.defaultInspectorTab;
      inspectorWidth = clampInspectorWidth(desktopPreferences.defaultInspectorWidth);
    }
  }

  function persistDesktopState() {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(storageKeys.prefs, JSON.stringify(desktopPreferences));
    if (desktopPreferences.rememberSession && selectedSession?.id) {
      window.localStorage.setItem(storageKeys.sessionId, selectedSession.id);
    } else if (!desktopPreferences.rememberSession) {
      window.localStorage.removeItem(storageKeys.sessionId);
    }
    if (desktopPreferences.rememberInspectorLayout) {
      window.localStorage.setItem(storageKeys.inspectorOpen, String(inspectorOpen));
      window.localStorage.setItem(storageKeys.inspectorTab, inspectorTab);
      window.localStorage.setItem(storageKeys.inspectorWidth, String(inspectorWidth));
    } else {
      window.localStorage.removeItem(storageKeys.inspectorOpen);
      window.localStorage.removeItem(storageKeys.inspectorTab);
      window.localStorage.removeItem(storageKeys.inspectorWidth);
    }
  }

  async function openSettingsView() {
    view = "settings";
    await refreshSettings();
  }

  async function refreshSettings() {
    settingsLoading = true;
    try {
      settingsSnapshot = await loadSettingsSnapshot(remoteConnection);
      if ((settingsSnapshot.auth?.length ?? 0) === 0) {
        view = "login";
      } else if (view === "login") {
        view = "workspace";
      }
      statusMessage = "Settings snapshot refreshed.";
    } catch (error) {
      statusMessage = String(error);
    } finally {
      settingsLoading = false;
    }
  }

  async function handleOauthLogin(providerId: string) {
    authBusyProviderId = providerId;
    authError = null;
    try {
      settingsSnapshot = await loginWithOauth(providerId, remoteConnection);
      view = "workspace";
      statusMessage = `Logged in to ${providerId}.`;
      await refreshGroups(preferredSessionId ?? undefined);
    } catch (error) {
      authError = String(error);
      statusMessage = authError;
    } finally {
      authBusyProviderId = null;
    }
  }

  async function handleApiKeyLogin(providerId: string, apiKey: string) {
    authBusyProviderId = providerId;
    authError = null;
    try {
      settingsSnapshot = await loginWithApiKey(providerId, apiKey, remoteConnection);
      view = "workspace";
      statusMessage = `Stored API key for ${providerId}.`;
      await refreshGroups(preferredSessionId ?? undefined);
    } catch (error) {
      authError = String(error);
      statusMessage = authError;
    } finally {
      authBusyProviderId = null;
    }
  }

  async function handleLogout(providerId: string) {
    authBusyProviderId = providerId;
    authError = null;
    try {
      settingsSnapshot = await logoutProvider(providerId, remoteConnection);
      statusMessage = `Logged out from ${providerId}.`;
      if ((settingsSnapshot.auth?.length ?? 0) === 0) {
        groups = [];
        selectedSession = null;
        sessionDetail = null;
        selectedItem = null;
        view = "login";
      }
    } catch (error) {
      authError = String(error);
      statusMessage = authError;
    } finally {
      authBusyProviderId = null;
    }
  }

  async function handleRemoteBash(command: string) {
    if (!remoteConnection.enabled) {
      return;
    }
    remoteBusy = true;
    try {
      remoteOperation = await runRemoteBash(remoteConnection, command);
      statusMessage = remoteOperation.success ? "Remote bash finished." : "Remote bash failed.";
    } catch (error) {
      statusMessage = String(error);
      remoteOperation = { success: false, stdout: "", stderr: String(error) };
    } finally {
      remoteBusy = false;
    }
  }

  async function handleRemoteRead(path: string) {
    if (!remoteConnection.enabled) {
      return;
    }
    remoteBusy = true;
    try {
      remoteOperation = await readRemoteFile(remoteConnection, path);
      statusMessage = remoteOperation.success ? `Read remote file ${path}.` : `Reading ${path} failed.`;
    } catch (error) {
      statusMessage = String(error);
      remoteOperation = { success: false, stdout: "", stderr: String(error) };
    } finally {
      remoteBusy = false;
    }
  }

  async function handleRemoteWrite(path: string, contents: string) {
    if (!remoteConnection.enabled) {
      return;
    }
    remoteBusy = true;
    try {
      remoteOperation = await writeRemoteFile(remoteConnection, path, contents);
      statusMessage = remoteOperation.success ? `Wrote remote file ${path}.` : `Writing ${path} failed.`;
    } catch (error) {
      statusMessage = String(error);
      remoteOperation = { success: false, stdout: "", stderr: String(error) };
    } finally {
      remoteBusy = false;
    }
  }

  function updateDesktopPreference<K extends keyof DesktopPreferences>(
    key: K,
    value: DesktopPreferences[K]
  ) {
    desktopPreferences = { ...desktopPreferences, [key]: value };
    if (key === "launchInspectorOpen" && !desktopPreferences.rememberInspectorLayout) {
      inspectorOpen = desktopPreferences.launchInspectorOpen;
    }
    if (key === "defaultInspectorTab" && !desktopPreferences.rememberInspectorLayout) {
      inspectorTab = desktopPreferences.defaultInspectorTab;
    }
    if (key === "defaultInspectorWidth" && !desktopPreferences.rememberInspectorLayout) {
      inspectorWidth = clampInspectorWidth(desktopPreferences.defaultInspectorWidth);
    }
    if (key === "rememberInspectorLayout" && !value) {
      inspectorOpen = desktopPreferences.launchInspectorOpen;
      inspectorTab = desktopPreferences.defaultInspectorTab;
      inspectorWidth = clampInspectorWidth(desktopPreferences.defaultInspectorWidth);
    }
  }

  function resetDesktopPreferences() {
    desktopPreferences = { ...defaultDesktopPreferences };
    preferredSessionId = null;
    inspectorOpen = desktopPreferences.launchInspectorOpen;
    inspectorTab = desktopPreferences.defaultInspectorTab;
    inspectorWidth = desktopPreferences.defaultInspectorWidth;
    if (typeof window !== "undefined") {
      window.localStorage.removeItem(storageKeys.sessionId);
      window.localStorage.removeItem(storageKeys.inspectorOpen);
      window.localStorage.removeItem(storageKeys.inspectorTab);
      window.localStorage.removeItem(storageKeys.inspectorWidth);
      window.localStorage.setItem(storageKeys.prefs, JSON.stringify(desktopPreferences));
    }
    statusMessage = "Desktop preferences reset.";
  }

  async function openSession(session: SessionListItem) {
    sessionLoading = true;
    try {
      const detail = await loadSessionDetail(session.id, remoteConnection);
      selectedSession = detail.session;
      sessionDetail = detail;
      syncInspectorForItem(chooseDefaultItem(detail.timeline));
      statusMessage = `Loaded ${detail.timeline.length} conversation items.`;
    } catch (error) {
      statusMessage = String(error);
    } finally {
      sessionLoading = false;
    }
  }

  async function refreshGroups(preferredSessionId?: string) {
    groupsLoading = true;
    try {
      groups = await listGroupedSessions(remoteConnection);
      const allSessions = groups.flatMap((group) => group.sessions);
      const selectedSessionId = selectedSession?.id ?? null;
      const nextSession =
        allSessions.find((session) => session.id === preferredSessionId) ??
        (selectedSessionId
          ? allSessions.find((session) => session.id === selectedSessionId)
          : null) ??
        allSessions[0] ??
        null;

      if (!nextSession) {
        selectedSession = null;
        sessionDetail = null;
        selectedItem = null;
        statusMessage = "No sessions found in this workspace yet.";
        return;
      }

      await openSession(nextSession);
    } catch (error) {
      statusMessage = String(error);
    } finally {
      groupsLoading = false;
    }
  }

  async function refreshSelectedRepo() {
    if (!selectedSession || !sessionDetail) {
      return;
    }
    actionBusy = true;
    try {
      const repoStatus = await refreshRepoStatus(selectedSession.id, remoteConnection);
      sessionDetail = { ...sessionDetail, repoStatus };
      statusMessage = "Repository status refreshed.";
    } catch (error) {
      statusMessage = String(error);
    } finally {
      actionBusy = false;
    }
  }

  async function runRepoAction(action: "create" | "merge") {
    if (!selectedSession || !sessionDetail) {
      return;
    }

    actionBusy = true;
    try {
      if (action === "create") {
        const defaults = buildPrDefaults(selectedSession);
        const result = await createPullRequest(
          selectedSession.id,
          defaults.title,
          defaults.body,
          remoteConnection
        );
        sessionDetail = { ...sessionDetail, repoStatus: result.repoStatus };
        statusMessage = result.message;
      } else {
        const result = await mergePullRequest(
          selectedSession.id,
          sessionDetail.repoStatus.pullRequest?.number,
          "merge",
          remoteConnection
        );
        sessionDetail = { ...sessionDetail, repoStatus: result.repoStatus };
        statusMessage = result.message;
      }
    } catch (error) {
      statusMessage = String(error);
    } finally {
      actionBusy = false;
    }
  }

  onMount(() => {
    restoreDesktopState();
    const handlePointerMove = (event: PointerEvent) => {
      if (!isResizingInspector) {
        return;
      }
      updateInspectorWidth(event.clientX);
    };
    const handlePointerUp = () => {
      if (!isResizingInspector) {
        return;
      }
      isResizingInspector = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    void (async () => {
      await refreshSettings();
      if (view !== "login") {
        await refreshGroups(preferredSessionId ?? undefined);
      }
    })();
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  });

  $: persistDesktopState();
</script>

<div class:single-column={view !== "workspace"} class="shell">
  {#if view === "workspace"}
    <SessionSidebar
      groups={groups}
      activeSessionId={selectedSession?.id ?? null}
      loading={groupsLoading}
      onSelect={(session) => void openSession(session)}
    />
  {/if}

  <main class="workspace">
    <HeaderBar
      session={selectedSession}
      repoStatus={sessionDetail?.repoStatus ?? null}
      {view}
      remoteLabel={remoteConnection.enabled ? remoteConnection.target : null}
      busy={actionBusy || sessionLoading}
      statusMessage={statusMessage}
      onRefresh={() => (view === "workspace" ? void refreshSelectedRepo() : void refreshSettings())}
      onCreatePr={() => void runRepoAction("create")}
      onMergePr={() => void runRepoAction("merge")}
      onOpenSettings={() => void openSettingsView()}
      onBackToWorkspace={() => (view = "workspace")}
    />

    {#if view === "workspace"}
      <OverviewStrip
        session={selectedSession}
        repoStatus={sessionDetail?.repoStatus ?? null}
        latestDiff={sessionDetail?.latestDiff ?? null}
        selectedItem={selectedItem}
        permissionCount={pendingPermissionCount}
        toolCount={toolCount}
        diffCount={diffCount}
        onOpenDiff={() => {
          inspectorOpen = true;
          inspectorTab = "latest-diff";
        }}
        onOpenHistory={() => {
          inspectorOpen = true;
          inspectorTab = "history";
        }}
        onOpenDetails={() => {
          inspectorOpen = true;
          inspectorTab = "tool-details";
        }}
      />

      <div
        bind:this={contentElement}
        class:loading={sessionLoading}
        class:resizing={isResizingInspector}
        class:single-pane={!inspectorOpen}
        class="content"
        style={`--inspector-width: ${inspectorWidth}%`}
      >
        <ConversationPane
          timeline={timeline}
          selectedId={selectedItem?.id ?? null}
          loading={sessionLoading}
          onSelect={(item) => syncInspectorForItem(item)}
        />

        {#if inspectorOpen}
          <button
            type="button"
            class="pane-divider"
            aria-label="Resize inspector"
            on:pointerdown={beginInspectorResize}
            on:keydown={nudgeInspector}
          >
            <span></span>
          </button>
        {/if}

        <InspectorPane
          open={inspectorOpen}
          tab={inspectorTab}
          latestDiff={sessionDetail?.latestDiff ?? null}
          diffHistory={sessionDetail?.diffHistory ?? []}
          timeline={timeline}
          selectedId={selectedItem?.id ?? null}
          selectedItem={selectedItem}
          onTabChange={(tab) => (inspectorTab = tab)}
          onToggle={() => (inspectorOpen = !inspectorOpen)}
          onSelectItem={(item) => syncInspectorForItem(item)}
        />

        {#if sessionLoading}
          <div class="loading-overlay">
            <div class="loading-card">
              <strong>Loading session</strong>
              <span>Refreshing transcript, diffs, and repository state.</span>
            </div>
          </div>
        {/if}
      </div>
    {:else if view === "settings"}
      <SettingsView
        snapshot={settingsSnapshot}
        loading={settingsLoading}
        preferences={desktopPreferences}
        remoteEnabled={remoteConnection.enabled}
        remotePassword={remotePassword}
        remoteBusy={remoteBusy}
        remoteResult={remoteOperation}
        onPreferenceChange={updateDesktopPreference}
        onRemotePasswordChange={(value) => (remotePassword = value)}
        onResetPreferences={resetDesktopPreferences}
        onRefresh={() => void refreshSettings()}
        onLogout={(providerId) => void handleLogout(providerId)}
        onRunRemoteBash={(command) => void handleRemoteBash(command)}
        onReadRemoteFile={(path) => void handleRemoteRead(path)}
        onWriteRemoteFile={(path, contents) => void handleRemoteWrite(path, contents)}
      />
    {:else}
      <LoginView
        snapshot={settingsSnapshot}
        loading={settingsLoading}
        remoteEnabled={remoteConnection.enabled}
        busyProviderId={authBusyProviderId}
        errorMessage={authError}
        onLoginOauth={(providerId) => void handleOauthLogin(providerId)}
        onLoginApiKey={(providerId, apiKey) => void handleApiKeyLogin(providerId, apiKey)}
        onRefresh={() => void refreshSettings()}
      />
    {/if}

    <footer class="statusbar">
      <div class="status-primary">
        <span>{groupsLoading ? "Loading sessions..." : statusMessage}</span>
        {#if selectedSession}
          <span class="status-meta">{selectedSession.cwd}</span>
        {/if}
      </div>
      <div class="status-secondary">
        <span class="status-pill">{inspectorOpen ? inspectorTab : "inspector collapsed"}</span>
        <span class="status-pill">{selectedLabel}</span>
        <span class="status-pill">{toolCount} tools</span>
        <span class="status-pill">{pendingPermissionCount} approvals</span>
        <span class="status-pill">{diffCount} diffs</span>
      </div>
    </footer>
  </main>
</div>
