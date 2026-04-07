<script lang="ts">
  import type { ProviderSummary, SettingsSnapshot } from "../types";

  export let snapshot: SettingsSnapshot | null = null;
  export let loading = false;
  export let remoteEnabled = false;
  export let busyProviderId: string | null = null;
  export let errorMessage: string | null = null;
  export let onLoginOauth: (providerId: string) => void = () => {};
  export let onLoginApiKey: (providerId: string, apiKey: string) => void = () => {};
  export let onRefresh: () => void = () => {};

  let apiKeys: Record<string, string> = {};

  function updateApiKey(providerId: string, value: string) {
    apiKeys = { ...apiKeys, [providerId]: value };
  }

  function submitApiKey(providerId: string) {
    onLoginApiKey(providerId, apiKeys[providerId] ?? "");
  }

  function supports(provider: ProviderSummary, mode: string): boolean {
    return provider.authModes.includes(mode);
  }
</script>

<section class="login-page">
  <div class="hero">
    <p class="eyebrow">Puffer Desktop</p>
    <h1>Sign in to start a session</h1>
    <p class="subcopy">
      The desktop shell needs provider credentials before it can open sessions, run agents, or create pull requests.
    </p>
    {#if remoteEnabled}
      <p class="subcopy">
        Remote mode is active. API keys are stored on the remote host, and OAuth opens in your local
        browser before the credential is written back to the remote host over SSH.
      </p>
    {/if}
    <button class="refresh" on:click={onRefresh}>Refresh auth state</button>
  </div>

  {#if errorMessage}
    <div class="error-banner">{errorMessage}</div>
  {/if}

  <div class="provider-grid">
    {#if loading}
      <div class="empty-card">Loading providers and auth state...</div>
    {:else if snapshot?.providers.length}
      {#each snapshot.providers as provider}
        <article class="provider-card">
          <div class="card-header">
            <div>
              <p class="eyebrow">Provider</p>
              <h2>{provider.displayName}</h2>
            </div>
            <span class="provider-meta">{provider.modelCount} models</span>
          </div>

          <p class="provider-copy">
            {provider.id} · {provider.defaultApi}
          </p>

          <div class="actions">
            {#if supports(provider, "oauth")}
              <button
                class="primary"
                disabled={busyProviderId === provider.id}
                on:click={() => onLoginOauth(provider.id)}
              >
                {busyProviderId === provider.id
                  ? "Opening browser..."
                  : remoteEnabled
                    ? "Login with OAuth (store remote)"
                    : "Login with OAuth"}
              </button>
            {/if}

            {#if supports(provider, "api_key")}
              <div class="api-key-row">
                <input
                  type="password"
                  value={apiKeys[provider.id] ?? ""}
                  placeholder="Paste API key"
                  on:input={(event) =>
                    updateApiKey(provider.id, (event.currentTarget as HTMLInputElement).value)}
                />
                <button
                  class="secondary"
                  disabled={busyProviderId === provider.id}
                  on:click={() => submitApiKey(provider.id)}
                >
                  Save key
                </button>
              </div>
            {/if}
          </div>

          <p class="hint">Supported auth: {provider.authModes.join(", ")}</p>
        </article>
      {/each}
    {:else}
      <div class="empty-card">No providers are registered in this workspace.</div>
    {/if}
  </div>
</section>

<style>
  .login-page {
    min-height: 0;
    overflow: auto;
    padding: 1.2rem;
    display: grid;
    gap: 1rem;
    background: rgba(255, 252, 246, 0.46);
  }

  .hero,
  .provider-card,
  .empty-card,
  .error-banner {
    border-radius: 24px;
    border: 1px solid rgba(111, 101, 89, 0.14);
    background: rgba(255, 255, 255, 0.76);
    box-shadow: var(--shadow-soft);
  }

  .hero {
    padding: 1.3rem 1.35rem;
    display: grid;
    gap: 0.45rem;
  }

  .eyebrow {
    margin: 0;
    font-size: 0.72rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  h1,
  h2 {
    margin: 0;
  }

  .subcopy {
    margin: 0;
    max-width: 48rem;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .refresh,
  .primary,
  .secondary {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 999px;
    padding: 0.65rem 0.9rem;
    cursor: pointer;
  }

  .primary {
    background: var(--accent);
    color: #fcfffd;
    border-color: var(--accent);
  }

  .secondary {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: rgba(20, 99, 86, 0.16);
  }

  .provider-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
  }

  .provider-card {
    padding: 1rem 1.05rem;
    display: grid;
    gap: 0.9rem;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
  }

  .provider-meta,
  .provider-copy,
  .hint {
    color: var(--text-muted);
  }

  .provider-copy,
  .hint {
    margin: 0;
    line-height: 1.45;
  }

  .actions {
    display: grid;
    gap: 0.75rem;
  }

  .api-key-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.65rem;
  }

  input {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.84);
    color: var(--text);
    padding: 0.78rem 0.92rem;
  }

  .empty-card,
  .error-banner {
    padding: 1rem;
    color: var(--text-muted);
  }

  .error-banner {
    background: rgba(247, 225, 220, 0.76);
    border-color: rgba(157, 58, 43, 0.16);
    color: var(--danger);
  }

  @media (max-width: 980px) {
    .provider-grid {
      grid-template-columns: 1fr;
    }

    .api-key-row {
      grid-template-columns: 1fr;
    }
  }
</style>
