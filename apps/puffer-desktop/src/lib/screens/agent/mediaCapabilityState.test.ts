import { expect, test } from "vitest";
import type { MediaCapabilityInfo, MediaKind } from "../../types";
import {
  availableMediaCapabilities,
  mediaCapabilityConnectStateMessage,
  unavailableMediaProviderLabels
} from "./mediaCapabilityState";

function capability(overrides: Partial<MediaCapabilityInfo> = {}): MediaCapabilityInfo {
  return {
    providerId: "relaydance",
    providerDisplayName: "Relaydance",
    modelId: "doubao-seedance-2-0-720p",
    modelDisplayName: "Seedance 2.0 (720p)",
    kind: "video",
    operation: "generate",
    adapter: "openai_video",
    parameters: [],
    defaults: {},
    status: "unavailable",
    source: "static",
    reason: "missing_auth",
    checkedAtMs: 42,
    ...overrides
  };
}

test("availableMediaCapabilities filters by kind and available status", () => {
  const capabilities = [
    capability({ status: "available", reason: null }),
    capability({ kind: "image" as MediaKind, status: "available", reason: null }),
    capability({ providerId: "xai", status: "unavailable", reason: "missing_auth" })
  ];

  expect(availableMediaCapabilities(capabilities, "video")).toEqual([
    capability({ status: "available", reason: null })
  ]);
});

test("unavailableMediaProviderLabels deduplicates provider display names", () => {
  const capabilities = [
    capability(),
    capability({ modelId: "another-model" }),
    capability({ providerId: "xai", providerDisplayName: "", reason: "missing_auth" })
  ];

  expect(unavailableMediaProviderLabels(capabilities, "video")).toEqual(["Relaydance", "xai"]);
});

test("mediaCapabilityConnectStateMessage appears only for unavailable video providers", () => {
  expect(mediaCapabilityConnectStateMessage([capability()], "video")).toBe(
    "Connect Relaydance to enable video generation."
  );
  expect(
    mediaCapabilityConnectStateMessage(
      [capability(), capability({ providerId: "xai", providerDisplayName: "xAI" })],
      "video"
    )
  ).toBe("Connect Relaydance or xAI to enable video generation.");
  expect(
    mediaCapabilityConnectStateMessage([capability({ status: "available", reason: null })], "video")
  ).toBeNull();
  expect(mediaCapabilityConnectStateMessage([capability({ kind: "image" })], "image")).toBeNull();
});
