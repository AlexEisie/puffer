import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { defineConfig, type ViteDevServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const workspaceDaemonHandshakeUrl = new URL("../../.puffer/daemon.handshake", import.meta.url);

const host =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env
    ?.TAURI_DEV_HOST ?? "127.0.0.1";

// Build commit, injected once at config load for the corner build badge.
// Degrades to "unknown" if git is unavailable (shallow CI checkout) rather than
// breaking the build.
function gitShortHash(): string {
  try {
    return execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function workspaceDaemonHandshake(): string | null {
  try {
    const raw = readFileSync(workspaceDaemonHandshakeUrl, { encoding: "utf8" }).trim();
    return raw ? raw : null;
  } catch {
    return null;
  }
}

function workspaceDaemonHandshakePlugin() {
  return {
    name: "puffer-workspace-daemon-handshake",
    configureServer(server: ViteDevServer) {
      server.middlewares.use("/__puffer/daemon-handshake", (_req, res) => {
        const raw = workspaceDaemonHandshake();
        if (!raw) {
          res.statusCode = 404;
          res.setHeader("content-type", "application/json");
          res.end("{}");
          return;
        }
        res.statusCode = 200;
        res.setHeader("cache-control", "no-store");
        res.setHeader("content-type", "application/json");
        res.end(raw);
      });
    }
  };
}

export default defineConfig({
  define: {
    __COMMIT_HASH__: JSON.stringify(gitShortHash())
  },
  plugins: [
    workspaceDaemonHandshakePlugin(),
    svelte({
      compilerOptions: {
        compatibility: {
          componentApi: 4
        }
      }
    })
  ],
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
  optimizeDeps: {
    entries: ["index.html"]
  },
  server: {
    host,
    port: 1420,
    strictPort: true,
    hmr: host !== "127.0.0.1"
      ? {
          protocol: "ws",
          host,
          port: 1421
        }
      : undefined
  },
  preview: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true
  }
});
