<script lang="ts">
  import type {
    DraftProxyEndpoint,
    NetworkProxySettings,
    ProxyScheme,
    SanitizedProxyEndpoint,
    SettingsSnapshot
  } from "../../types";
  import { saveProxySettings, testProxy } from "../../api/desktop";
  import Icon from "../../design/Icon.svelte";

  type Props = {
    snapshot: SettingsSnapshot | null;
    onSaved: (snapshot: SettingsSnapshot) => void;
  };

  let props: Props = $props();

  const defaultBypass = ["localhost", "127.0.0.1", "::1", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"];
  const emptyProxy: NetworkProxySettings = {
    enabled: false,
    selected: null,
    bypass: defaultBypass,
    proxies: [],
    lastTest: null
  };
  const schemes: ProxyScheme[] = ["http", "https", "socks5", "socks5h"];

  let saving = $state(false);
  let testingId = $state<string | null>(null);
  let error = $state<string | null>(null);
  let bypassDraft = $state(defaultBypass.join("\n"));
  let lastTest = $state<NetworkProxySettings["lastTest"]>(null);
  let editing = $state<DraftProxyEndpoint | null>(null);
  let draftPassword = $state("");

  let proxy = $derived(props.snapshot?.networkProxy ?? emptyProxy);
  $effect(() => {
    if (!props.snapshot?.networkProxy) return;
    const nextBypass = props.snapshot.networkProxy.bypass.join("\n");
    if (nextBypass !== bypassDraft) {
      bypassDraft = nextBypass;
    }
    lastTest = props.snapshot.networkProxy.lastTest;
  });

  function nextProxyId() {
    return `proxy-${Date.now()}`;
  }

  function endpointUri(endpoint: DraftProxyEndpoint) {
    return `${endpoint.scheme}://${endpoint.host.trim()}:${endpoint.port || 0}`;
  }

  function toSaveProxySettingsInput(next: NetworkProxySettings) {
    return {
      enabled: next.enabled,
      selected: next.selected,
      bypass: next.bypass,
      proxies: next.proxies.map((item) => ({
        id: item.id,
        scheme: item.scheme,
        host: item.host,
        port: item.port,
        username: item.username,
        password: null,
        keepPassword: item.hasPassword
      }))
    };
  }

  function normalizeBypass(value: string) {
    return value
      .split(/\r?\n|,/)
      .map((entry) => entry.trim())
      .filter(Boolean);
  }

  function endpointToDraft(item: SanitizedProxyEndpoint): DraftProxyEndpoint {
    return {
      id: item.id,
      scheme: item.scheme,
      host: item.host,
      port: item.port,
      username: item.username,
      password: null,
      keepPassword: item.hasPassword
    };
  }

  function draftToSanitized(endpoint: DraftProxyEndpoint, existing?: SanitizedProxyEndpoint): SanitizedProxyEndpoint {
    return {
      id: endpoint.id,
      scheme: endpoint.scheme,
      host: endpoint.host.trim(),
      port: endpoint.port,
      username: endpoint.username?.trim() || null,
      hasPassword: Boolean(endpoint.password?.length || endpoint.keepPassword || existing?.hasPassword),
      uri: endpointUri(endpoint)
    };
  }

  async function persist(next: NetworkProxySettings) {
    saving = true;
    error = null;
    try {
      const input = toSaveProxySettingsInput(next);
      const saved = await saveProxySettings(input);
      lastTest = saved.networkProxy.lastTest;
      props.onSaved(saved);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function addProxy() {
    error = null;
    draftPassword = "";
    editing = {
      id: nextProxyId(),
      scheme: "socks5h",
      host: "127.0.0.1",
      port: 7890,
      username: null,
      password: null,
      keepPassword: false
    };
  }

  function editProxy(item: SanitizedProxyEndpoint) {
    error = null;
    draftPassword = "";
    editing = endpointToDraft(item);
  }

  function closeEditor() {
    editing = null;
    draftPassword = "";
  }

  async function saveEditingProxy() {
    if (!editing) return;
    const nextDraft: DraftProxyEndpoint = {
      ...editing,
      id: editing.id.trim(),
      host: editing.host.trim(),
      username: editing.username?.trim() || null,
      password: draftPassword.trim() ? draftPassword : null,
      keepPassword: !draftPassword.trim() && Boolean(editing.keepPassword)
    };
    const existing = proxy.proxies.find((item) => item.id === nextDraft.id);
    if (!nextDraft.id || !nextDraft.host || !nextDraft.port) {
      error = "Proxy id, host, and port are required.";
      return;
    }
    const nextItem = draftToSanitized(nextDraft, existing);
    const nextProxies = proxy.proxies.some((item) => item.id === nextItem.id)
      ? proxy.proxies.map((item) => (item.id === nextItem.id ? nextItem : item))
      : [...proxy.proxies, nextItem];
    const nextSelected = proxy.selected ?? nextItem.id;
    const input = {
      enabled: proxy.enabled,
      selected: nextSelected,
      bypass: proxy.bypass,
      proxies: nextProxies.map((item) => ({
        id: item.id,
        scheme: item.scheme,
        host: item.host,
        port: item.port,
        username: item.username,
        password: item.id === nextDraft.id ? nextDraft.password : null,
        keepPassword: item.id === nextDraft.id ? Boolean(nextDraft.keepPassword) : item.hasPassword
      }))
    };
    saving = true;
    error = null;
    try {
      const saved = await saveProxySettings(input);
      lastTest = saved.networkProxy.lastTest;
      props.onSaved(saved);
      closeEditor();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function testSavedProxy(proxyId: string) {
    testingId = proxyId;
    error = null;
    try {
      lastTest = await testProxy({ proxyId });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      testingId = null;
    }
  }

  function proxyStatusLabel(proxyId: string) {
    if (testingId === proxyId) return "checking...";
    if (lastTest?.proxyId !== proxyId) return null;
    if (lastTest.ok) {
      return lastTest.latencyMs === null
        ? "connected"
        : `connected (ping: ${lastTest.latencyMs} ms)`;
    }
    return "failed";
  }

  function proxyStatusState(proxyId: string) {
    if (testingId === proxyId) return "checking";
    if (lastTest?.proxyId !== proxyId) return "unknown";
    return lastTest.ok ? "connected" : "failed";
  }

  function proxyStatusTitle(proxyId: string) {
    return lastTest?.proxyId === proxyId ? lastTest.message : "";
  }

  function saveBypass() {
    void persist({ ...proxy, bypass: normalizeBypass(bypassDraft) });
  }

  function resetBypassDefaults() {
    bypassDraft = defaultBypass.join("\n");
    void persist({ ...proxy, bypass: defaultBypass });
  }
</script>

<h2>Network</h2>
<p class="lead">Provider proxy configuration for model, discovery, and OAuth requests.</p>

{#if error}
  <div class="pf-settings-note warn">{error}</div>
{/if}

<div class="pf-settings-row">
  <div class="meta">
    <div class="label">Proxy</div>
    <div class="desc">Route provider traffic through the selected proxy.</div>
  </div>
  <input
    type="checkbox"
    class="sc-switch"
    checked={proxy.enabled}
    disabled={saving}
    onchange={(e) => persist({ ...proxy, enabled: (e.currentTarget as HTMLInputElement).checked })}
  />
</div>

<section class="pf-network-section" aria-label="Proxy list">
  <div class="pf-network-section-head">
    <div>
      <h3>Proxy list</h3>
      <p>Saved endpoints available for provider requests.</p>
    </div>
  </div>
  <div class="pf-network-list">
    {#if proxy.proxies.length === 0}
      <div class="pf-empty">No proxies added.</div>
    {:else}
      {#each proxy.proxies as item (item.id)}
        <article class="pf-network-proxy-card" data-selected={proxy.selected === item.id}>
          <label class="pf-network-proxy-main">
            <input
              type="radio"
              name="proxy"
              checked={proxy.selected === item.id}
              disabled={saving}
              onchange={() => persist({ ...proxy, selected: item.id })}
            />
            <span>
              <strong>{item.uri}</strong>
              {#if proxyStatusLabel(item.id)}
                <small class="pf-network-status" data-state={proxyStatusState(item.id)} title={proxyStatusTitle(item.id)}>
                  {proxyStatusLabel(item.id)}
                </small>
              {/if}
              {#if item.username}
                <small class="pf-network-username">{item.username}</small>
              {/if}
            </span>
          </label>
          <div class="pf-network-proxy-actions">
            <button type="button" class="sc-btn" data-variant="outline" data-size="sm" disabled={testingId !== null} onclick={() => testSavedProxy(item.id)}>
              <Icon name="test" size={12} />{testingId === item.id ? "Testing..." : "Test"}
            </button>
            <button type="button" class="sc-btn" data-variant="outline" data-size="sm" disabled={saving} onclick={() => editProxy(item)}>
              <Icon name="edit" size={12} />Edit
            </button>
          </div>
        </article>
      {/each}
    {/if}
  </div>
  <button type="button" class="sc-btn pf-network-add" data-variant="outline" data-size="sm" disabled={saving} onclick={addProxy}>
    <Icon name="plus" size={12} />Add proxy
  </button>
</section>

<section class="pf-network-section" aria-label="Bypass">
  <div class="pf-network-section-head">
    <div>
      <h3>Bypass</h3>
      <p>Hosts, IPs, and CIDR ranges that should skip the proxy.</p>
    </div>
  </div>
  <textarea class="sc-input pf-network-bypass" bind:value={bypassDraft}></textarea>
  <div class="pf-network-actions">
    <button type="button" class="sc-btn" data-variant="outline" data-size="sm" disabled={saving} onclick={resetBypassDefaults}>
      Reset defaults
    </button>
    <button type="button" class="sc-btn" data-size="sm" disabled={saving} onclick={saveBypass}>
      Save bypass
    </button>
  </div>
</section>

{#if editing}
  <div class="pf-network-modal-scrim" role="presentation" onclick={closeEditor} onkeydown={() => {}}>
    <div
      class="pf-network-modal"
      role="dialog"
      aria-label="Edit proxy"
      aria-modal="true"
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          closeEditor();
        }
      }}
    >
      <div class="pf-network-modal-head">
        <div>
          <h3>{proxy.proxies.some((item) => item.id === editing?.id) ? "Edit proxy" : "Add proxy"}</h3>
          <p>Credentials are saved to config and never shown in snapshots.</p>
        </div>
        <button type="button" class="pf-network-modal-close" aria-label="Close" onclick={closeEditor}>
          <Icon name="x" size={14} />
        </button>
      </div>
      <div class="pf-network-form">
        <label>
          Id
          <input class="sc-input" value={editing.id} disabled={saving} oninput={(e) => (editing = { ...editing!, id: (e.currentTarget as HTMLInputElement).value })} />
        </label>
        <label>
          Scheme
          <select class="sc-input" value={editing.scheme} disabled={saving} onchange={(e) => (editing = { ...editing!, scheme: (e.currentTarget as HTMLSelectElement).value as ProxyScheme })}>
            {#each schemes as scheme}
              <option value={scheme}>{scheme}</option>
            {/each}
          </select>
        </label>
        <label>
          Host
          <input class="sc-input" value={editing.host} disabled={saving} oninput={(e) => (editing = { ...editing!, host: (e.currentTarget as HTMLInputElement).value })} />
        </label>
        <label>
          Port
          <input class="sc-input" type="number" min="1" max="65535" value={editing.port} disabled={saving} oninput={(e) => (editing = { ...editing!, port: Number((e.currentTarget as HTMLInputElement).value) })} />
        </label>
        <label>
          Username
          <input class="sc-input" value={editing.username ?? ""} disabled={saving} oninput={(e) => (editing = { ...editing!, username: (e.currentTarget as HTMLInputElement).value || null })} />
        </label>
        <label>
          Password
          <input class="sc-input" type="password" value={draftPassword} disabled={saving} placeholder={editing.keepPassword ? "Stored password unchanged" : ""} oninput={(e) => (draftPassword = (e.currentTarget as HTMLInputElement).value)} />
        </label>
      </div>
      <div class="pf-network-modal-foot">
        <button type="button" class="sc-btn" data-variant="outline" data-size="sm" disabled={saving} onclick={closeEditor}>
          Cancel
        </button>
        <button type="button" class="sc-btn" data-size="sm" disabled={saving} onclick={saveEditingProxy}>
          {saving ? "Saving..." : "Save proxy"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .pf-network-section {
    margin-top: 22px;
  }

  .pf-network-section-head {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 10px;
  }

  .pf-network-section-head h3 {
    margin: 0;
  }

  .pf-network-section-head p {
    margin: 3px 0 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .pf-network-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .pf-network-proxy-card {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--background);
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 12px 14px;
  }

  .pf-network-proxy-card[data-selected="true"] {
    border-color: color-mix(in oklab, var(--puffer-accent) 42%, var(--border));
    background: color-mix(in oklab, var(--puffer-accent) 5%, var(--background));
  }

  .pf-network-proxy-main {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }

  .pf-network-proxy-main span {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pf-network-proxy-main strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-family: var(--font-mono);
  }

  .pf-network-status,
  .pf-network-username {
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .pf-network-status {
    font-family: var(--font-mono);
  }

  .pf-network-status[data-state="connected"] {
    color: oklch(0.46 0.14 145);
  }

  .pf-network-status[data-state="failed"] {
    color: var(--pf-run-failed);
  }

  .pf-network-status[data-state="checking"] {
    color: var(--puffer-accent);
  }

  .pf-network-proxy-actions,
  .pf-network-actions,
  .pf-network-modal-foot {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }

  .pf-network-add {
    margin-top: 10px;
  }

  .pf-network-bypass {
    width: 100%;
    min-height: 116px;
    resize: vertical;
    font-family: var(--font-mono);
    line-height: 1.45;
  }

  .pf-network-actions {
    justify-content: flex-end;
    margin-top: 10px;
  }

  .pf-network-modal-scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: color-mix(in oklab, var(--background) 54%, transparent);
    backdrop-filter: blur(8px);
  }

  .pf-network-modal {
    width: min(560px, calc(100vw - 32px));
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--background);
    box-shadow: 0 24px 70px color-mix(in oklab, var(--foreground) 18%, transparent);
    overflow: hidden;
  }

  .pf-network-modal-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .pf-network-modal-head h3,
  .pf-network-modal-head p {
    margin: 0;
  }

  .pf-network-modal-head p {
    margin-top: 3px;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .pf-network-modal-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
  }

  .pf-network-modal-close:hover {
    background: var(--pf-selected-bg-hover);
    color: var(--foreground);
  }

  .pf-network-form {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    padding: 14px 16px;
  }

  .pf-network-form label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .pf-network-modal-foot {
    justify-content: flex-end;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }

  @media (max-width: 760px) {
    .pf-network-proxy-card {
      grid-template-columns: 1fr;
    }

    .pf-network-proxy-actions,
    .pf-network-actions,
    .pf-network-modal-foot {
      justify-content: flex-start;
    }

    .pf-network-form {
      grid-template-columns: 1fr;
    }
  }
</style>
