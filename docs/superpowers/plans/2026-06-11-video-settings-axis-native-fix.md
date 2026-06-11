# Video Settings Axis-Native Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make desktop image/video generation settings consume daemon axis-native capabilities and save logical media selections without the old `parameters/defaults` shape.

**Architecture:** Keep the protocol boundary simple: daemon capabilities provide ordered `axes`, desktop renders those axes generically, and config writes `{ providerId, logicalModelId, selections }`. Add a small TypeScript helper for axis parsing/normalization so `MediaSettingsModal.svelte` stays focused on UI state and save flow.

**Tech Stack:** Svelte 5, TypeScript, Vitest, Playwright, Rust daemon unit tests.

---

## Scope Guard

This plan intentionally does not add a shared DTO crate, provider-specific video forms, or a Tauri backend convergence project. Touch Tauri backend code only if a focused test proves the desktop execution path still emits old `parameters/defaults` fields. The old persisted concrete shape is not preserved; it is replaced by logical media settings in desktop request payloads.

## File Structure

- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts`
  - Owns parsing of enum/range/bool axis controls, default extraction, validation, and saved-selection normalization.
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts`
  - Covers helper behavior independent of Svelte rendering.
- Modify: `apps/puffer-desktop/src/lib/types.ts`
  - Replace old media settings and capability types with axis-native logical types.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
  - Replace parameter/default-specific state with axis selection state.
  - Render axes in daemon order.
  - Save logical settings only.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`
  - Update test fixtures to the new capability shape.
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`
  - Make fake daemon capabilities/settings axis-native.
  - Normalize only `{ providerId, logicalModelId, selections }`.
  - Match generated media requests by logical model id.
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Update media test helpers, saved settings fixtures, and expected `update_config` payloads.
  - Keep focused video modal coverage proving `axes` payloads do not crash and save logical settings.
- Modify: `crates/puffer-cli/src/daemon.rs`
  - Strengthen existing daemon media capability tests to assert `axes` exists and old fields are absent.

## Tasks

### Task 1: Add Axis Helper Tests And Implementation

**Files:**
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts`
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts`

- [ ] **Step 1: Write the failing helper tests**

Create `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts`:

```ts
import { expect, test } from "vitest";
import {
  axisControlKind,
  axisDefaultValue,
  axisOptions,
  normalizeAxisSelections,
  selectionIsValid
} from "./mediaAxisControls";

const enumAxis = {
  id: "aspect_ratio",
  label: "Aspect ratio",
  role: "param",
  control: { enum: { values: ["16:9", "9:16"], default: "16:9" } },
  requestField: "metadata.ratio",
  wireType: "string"
};

const rangeAxis = {
  id: "duration_seconds",
  label: "Duration",
  role: "param",
  control: { range: { min: 4, max: 12, step: 2, default: 6 } },
  requestField: "seconds",
  wireType: "number"
};

const boolAxis = {
  id: "audio",
  label: "Native audio",
  role: "selector",
  control: { bool: { default: true } },
  requestField: null,
  wireType: "string"
};

test("axis helpers expose enum metadata", () => {
  expect(axisControlKind(enumAxis)).toBe("enum");
  expect(axisOptions(enumAxis)).toEqual(["16:9", "9:16"]);
  expect(axisDefaultValue(enumAxis)).toBe("16:9");
  expect(selectionIsValid(enumAxis, "9:16")).toBe(true);
  expect(selectionIsValid(enumAxis, "1:1")).toBe(false);
});

test("axis helpers normalize bool and range values to strings", () => {
  expect(axisControlKind(rangeAxis)).toBe("range");
  expect(axisDefaultValue(rangeAxis)).toBe("6");
  expect(selectionIsValid(rangeAxis, "8")).toBe(true);
  expect(selectionIsValid(rangeAxis, "9")).toBe(false);
  expect(axisControlKind(boolAxis)).toBe("bool");
  expect(axisDefaultValue(boolAxis)).toBe("true");
  expect(selectionIsValid(boolAxis, "false")).toBe(true);
});

test("normalizeAxisSelections keeps valid saved values and drops stale keys", () => {
  expect(
    normalizeAxisSelections([enumAxis, rangeAxis, boolAxis], {
      aspect_ratio: "9:16",
      duration_seconds: "20",
      audio: "false",
      stale_video_option: "remove-me"
    })
  ).toEqual({
    aspect_ratio: "9:16",
    duration_seconds: "6",
    audio: "false"
  });
});

test("malformed controls are invalid so the modal can block saving", () => {
  const malformedAxis = {
    id: "resolution",
    label: "Resolution",
    role: "param",
    control: { enum: { values: [], default: "" } },
    requestField: "metadata.resolution",
    wireType: "string"
  };

  expect(axisControlKind(malformedAxis)).toBe("invalid");
  expect(axisDefaultValue(malformedAxis)).toBeNull();
  expect(axisOptions(malformedAxis)).toEqual([]);
  expect(selectionIsValid(malformedAxis, "720p")).toBe(false);
});
```

- [ ] **Step 2: Run the helper tests and confirm they fail**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaAxisControls.test.ts
```

Expected: FAIL because `./mediaAxisControls` does not exist.

- [ ] **Step 3: Add the axis helper implementation**

Create `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts`:

```ts
export type AxisControlKind = "enum" | "range" | "bool" | "invalid";

type MediaCapabilityAxisLike = {
  id: string;
  control: unknown;
};

type EnumControl = { enum: { values: string[]; default: string } };
type RangeControl = { range: { min: number; max: number; step: number; default: number } };
type BoolControl = { bool: { default: boolean } };

export function axisControlKind(axis: Pick<MediaCapabilityAxisLike, "control">): AxisControlKind {
  if (enumControl(axis)) return "enum";
  if (rangeControl(axis)) return "range";
  if (boolControl(axis)) return "bool";
  return "invalid";
}

export function axisOptions(axis: Pick<MediaCapabilityAxisLike, "control">): string[] {
  const control = enumControl(axis);
  return control ? [...control.enum.values] : [];
}

export function axisDefaultValue(axis: Pick<MediaCapabilityAxisLike, "control">): string | null {
  const enumValue = enumControl(axis);
  if (enumValue) return enumValue.enum.default;
  const rangeValue = rangeControl(axis);
  if (rangeValue) return String(rangeValue.range.default);
  const boolValue = boolControl(axis);
  if (boolValue) return boolValue.bool.default ? "true" : "false";
  return null;
}

export function selectionIsValid(
  axis: Pick<MediaCapabilityAxisLike, "control">,
  value: string | undefined
): value is string {
  if (value === undefined) return false;
  const enumValue = enumControl(axis);
  if (enumValue) return enumValue.enum.values.includes(value);
  const boolValue = boolControl(axis);
  if (boolValue) return value === "true" || value === "false";
  const rangeValue = rangeControl(axis);
  if (!rangeValue) return false;
  const numeric = Number(value);
  const { min, max, step } = rangeValue.range;
  if (!Number.isFinite(numeric) || numeric < min || numeric > max) return false;
  const offset = (numeric - min) / step;
  return Math.abs(offset - Math.round(offset)) < 1e-9;
}

export function normalizeAxisSelections(
  axes: MediaCapabilityAxisLike[],
  saved: Record<string, string>
): Record<string, string> {
  const next: Record<string, string> = {};
  for (const axis of axes) {
    if (selectionIsValid(axis, saved[axis.id])) {
      next[axis.id] = saved[axis.id];
      continue;
    }
    const defaultValue = axisDefaultValue(axis);
    if (defaultValue !== null && selectionIsValid(axis, defaultValue)) {
      next[axis.id] = defaultValue;
    }
  }
  return next;
}

export function capabilityAxesError(axes: MediaCapabilityAxisLike[]): string | null {
  if (!Array.isArray(axes)) return "Capability axes are malformed.";
  for (const axis of axes) {
    if (!axis.id || axisControlKind(axis) === "invalid") {
      return `Capability axis ${axis.id || "(missing id)"} is malformed.`;
    }
  }
  return null;
}

function enumControl(axis: Pick<MediaCapabilityAxisLike, "control">): EnumControl | null {
  const control = axis.control as Partial<EnumControl> | null | undefined;
  const enumValue = control?.enum;
  if (!enumValue || !Array.isArray(enumValue.values)) return null;
  if (enumValue.values.length === 0) return null;
  if (!enumValue.values.every((value) => typeof value === "string" && value.length > 0)) {
    return null;
  }
  if (typeof enumValue.default !== "string" || !enumValue.values.includes(enumValue.default)) {
    return null;
  }
  return { enum: { values: enumValue.values, default: enumValue.default } };
}

function rangeControl(axis: Pick<MediaCapabilityAxisLike, "control">): RangeControl | null {
  const control = axis.control as Partial<RangeControl> | null | undefined;
  const rangeValue = control?.range;
  if (!rangeValue) return null;
  const { min, max, step, default: defaultValue } = rangeValue;
  if (![min, max, step, defaultValue].every(Number.isFinite)) return null;
  if (max < min || step <= 0 || defaultValue < min || defaultValue > max) return null;
  return { range: { min, max, step, default: defaultValue } };
}

function boolControl(axis: Pick<MediaCapabilityAxisLike, "control">): BoolControl | null {
  const control = axis.control as Partial<BoolControl> | null | undefined;
  const boolValue = control?.bool;
  if (!boolValue || typeof boolValue.default !== "boolean") return null;
  return { bool: { default: boolValue.default } };
}
```

- [ ] **Step 4: Run the helper tests and confirm they pass**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaAxisControls.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit the helper**

```bash
git add apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts
git commit -m "test: add media axis selection helpers"
```

### Task 2: Replace Desktop Media Types With Axis-Native Types

**Files:**
- Modify: `apps/puffer-desktop/src/lib/types.ts`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`

- [ ] **Step 1: Update the type tests first**

In `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`, replace the `capability()` fixture body with:

```ts
function capability(overrides: Partial<MediaCapabilityInfo> = {}): MediaCapabilityInfo {
  return {
    providerId: "relaydance",
    providerDisplayName: "Relaydance",
    modelId: "doubao-seedance-2-0-720p",
    modelDisplayName: "Seedance 2.0 (720p)",
    kind: "video",
    operation: "generate",
    adapter: "relaydance_video",
    axes: [
      {
        id: "resolution",
        label: "Resolution",
        role: "param",
        control: { enum: { values: ["720p"], default: "720p" } },
        requestField: "metadata.resolution",
        wireType: "string"
      }
    ],
    status: "unavailable",
    source: "static",
    reason: "missing_auth",
    checkedAtMs: 42,
    ...overrides
  };
}
```

- [ ] **Step 2: Run the affected type tests and confirm they fail**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaCapabilityState.test.ts src/lib/screens/agent/mediaAxisControls.test.ts
```

Expected: FAIL because `MediaCapabilityInfo` still requires `parameters/defaults` and does not define `axes`.

- [ ] **Step 3: Update desktop media types**

In `apps/puffer-desktop/src/lib/types.ts`, replace the old `MediaGenerationSettings`, `MediaCapabilityInfo`, and `MediaCapabilityParameterInfo` block with:

```ts
export type MediaGenerationSettings = {
  providerId: string;
  logicalModelId: string;
  selections: Record<string, string>;
};

export type MediaKind = "image" | "video";

export type GenerateMediaInput = {
  kind: MediaKind;
  prompt: string;
  count?: number;
};

export type MediaCapabilityControl =
  | { enum: { values: string[]; default: string } }
  | { range: { min: number; max: number; step: number; default: number } }
  | { bool: { default: boolean } };

export type MediaCapabilityAxisInfo = {
  id: string;
  label: string;
  role: "param" | "selector" | string;
  control: MediaCapabilityControl;
  requestField: string | null;
  wireType: "string" | "number" | string;
};

export type MediaCapabilityInfo = {
  providerId: string;
  providerDisplayName: string;
  modelId: string;
  modelDisplayName: string;
  kind: MediaKind;
  operation: string;
  adapter?: string;
  axes: MediaCapabilityAxisInfo[];
  status: "available" | "unavailable" | "unknown" | string;
  source: string;
  reason: string | null;
  checkedAtMs: number;
};
```

- [ ] **Step 4: Run the focused TypeScript tests**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaCapabilityState.test.ts src/lib/screens/agent/mediaAxisControls.test.ts
```

Expected: PASS.

- [ ] **Step 5: Keep this red state uncommitted**

Do not commit after this task. `MediaSettingsModal.svelte`, `fakeDaemon.ts`, and Playwright fixtures still use the old type shape. Task 5 commits the complete desktop migration after the focused desktop tests pass.

### Task 3: Move Fake Daemon To Logical Media Settings

**Files:**
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`

- [ ] **Step 1: Add a failing request-shape assertion to the fake daemon**

In `apps/puffer-desktop/tests/support/fakeDaemon.ts`, replace `normalizeMediaSelection()` with this stricter version before changing the rest of the fake daemon:

```ts
function normalizeMediaSelection(value: unknown): FakeMediaSelection | null {
  if (!value || typeof value !== "object") return null;
  const record = value as JsonRecord;
  if (
    typeof record.providerId !== "string" ||
    typeof record.logicalModelId !== "string" ||
    record.modelId !== undefined ||
    record.adapter !== undefined ||
    record.parameters !== undefined ||
    !record.selections ||
    typeof record.selections !== "object" ||
    Array.isArray(record.selections)
  ) {
    return null;
  }
  return {
    providerId: record.providerId,
    logicalModelId: record.logicalModelId,
    selections: normalizeStringRecord(record.selections)
  };
}
```

- [ ] **Step 2: Run one existing media Playwright test and confirm it fails**

Run from `apps/puffer-desktop`:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer video generation settings saves configurable video defaults"
```

Expected: FAIL because the UI still sends `modelId/adapter/parameters`, causing fake daemon media config to normalize to `null`.

- [ ] **Step 3: Replace fake daemon media types and cloning**

In `apps/puffer-desktop/tests/support/fakeDaemon.ts`, replace the fake media type block with:

```ts
type FakeMediaSelection = {
  providerId: string;
  logicalModelId: string;
  selections: Record<string, string>;
};

type FakeMediaSettings = {
  image: FakeMediaSelection | null;
  video: FakeMediaSelection | null;
};

export type FakeMediaAxisControl =
  | { enum: { values: string[]; default: string } }
  | { range: { min: number; max: number; step: number; default: number } }
  | { bool: { default: boolean } };

export type FakeMediaCapabilityAxis = {
  id: string;
  label: string;
  role: "param" | "selector" | string;
  control: FakeMediaAxisControl;
  requestField: string | null;
  wireType: "string" | "number" | string;
};

export type FakeMediaCapability = {
  providerId: string;
  providerDisplayName: string;
  modelId: string;
  modelDisplayName: string;
  kind: "image" | "video";
  operation: string;
  adapter?: string;
  axes: FakeMediaCapabilityAxis[];
  status: string;
  source: string;
  reason: string | null;
  checkedAtMs: number;
};
```

Replace `cloneMediaSelection()` and `cloneMediaCapability()` with:

```ts
function cloneMediaSelection(selection: FakeMediaSelection | null): FakeMediaSelection | null {
  return selection ? { ...selection, selections: { ...selection.selections } } : null;
}

function cloneMediaCapability(capability: FakeMediaCapability): FakeMediaCapability {
  return {
    ...capability,
    axes: capability.axes.map((axis) => ({
      ...axis,
      control: cloneMediaAxisControl(axis.control)
    }))
  };
}

function cloneMediaAxisControl(control: FakeMediaAxisControl): FakeMediaAxisControl {
  if ("enum" in control) {
    return { enum: { values: [...control.enum.values], default: control.enum.default } };
  }
  if ("range" in control) {
    return { range: { ...control.range } };
  }
  return { bool: { ...control.bool } };
}
```

- [ ] **Step 4: Replace fake daemon default capabilities**

In `defaultFakeMediaCapabilities()`, replace the image capability entry with:

```ts
{
  providerId: "openai",
  providerDisplayName: "OpenAI",
  modelId: "gpt-image-1",
  modelDisplayName: "GPT Image 1",
  kind: "image",
  operation: "generate",
  adapter: "images_json",
  axes: [
    {
      id: "size",
      label: "Size",
      role: "param",
      control: { enum: { values: ["1024x1024", "1024x1536", "1536x1024"], default: "1024x1024" } },
      requestField: "size",
      wireType: "string"
    },
    {
      id: "quality",
      label: "Quality",
      role: "param",
      control: { enum: { values: ["auto", "low", "medium", "high"], default: "auto" } },
      requestField: "quality",
      wireType: "string"
    },
    {
      id: "output_format",
      label: "Output format",
      role: "param",
      control: { enum: { values: ["png", "jpeg", "webp"], default: "png" } },
      requestField: "output_format",
      wireType: "string"
    }
  ],
  status: "available",
  source: "fake-daemon",
  reason: null,
  checkedAtMs: now
}
```

- [ ] **Step 5: Match generated media by logical model id**

In `generateMedia()`, replace the settings check and capability lookup with:

```ts
const settings = this.settingsConfig.media[kind];
if (!settings) {
  throw new Error(`${kind} media provider/model is not configured.`);
}
const capability = capabilities.find(
  (item) =>
    item.providerId === settings.providerId &&
    item.modelId === settings.logicalModelId
);
if (!capability) {
  throw new Error(
    `selected ${kind} model unavailable: ${settings.providerId}/${settings.logicalModelId}`
  );
}
```

In the returned fixture fallback, replace `settings.modelId` with `settings.logicalModelId`.

- [ ] **Step 6: Run the focused Playwright test and keep the expected failure**

Run from `apps/puffer-desktop`:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer video generation settings saves configurable video defaults"
```

Expected: FAIL because the UI and Playwright fixtures still use old `parameters/defaults`.

- [ ] **Step 7: Keep this red state uncommitted**

Do not commit after this task. `apps/puffer-desktop/tests/chat-session-ui.spec.ts` still imports the old fake capability shape, so Task 4 completes the same test surface and commits both files together.

### Task 4: Update Playwright Media Fixtures To Axis Payloads

**Files:**
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [ ] **Step 1: Replace top-level media test helpers**

In `apps/puffer-desktop/tests/chat-session-ui.spec.ts`, replace `configuredImageMedia`, `FakeMediaCapabilityParameter`, `mediaParameter`, `imageCapability`, and `videoCapability` with:

```ts
const configuredImageMedia: MediaSettings = {
  image: {
    providerId: "openai",
    logicalModelId: "gpt-image-1",
    selections: {
      size: "1024x1024",
      quality: "auto",
      output_format: "png"
    }
  },
  video: null
};

type FakeMediaCapabilityAxis = FakeMediaCapability["axes"][number];

function mediaEnumAxis(input: {
  id: string;
  label: string;
  values: string[];
  defaultValue: string;
  requestField?: string | null;
  wireType?: "string" | "number";
}): FakeMediaCapabilityAxis {
  return {
    id: input.id,
    label: input.label,
    role: "param",
    control: { enum: { values: input.values, default: input.defaultValue } },
    requestField: input.requestField === undefined ? input.id : input.requestField,
    wireType: input.wireType ?? "string"
  };
}

function mediaRangeAxis(input: {
  id: string;
  label: string;
  min: number;
  max: number;
  step: number;
  defaultValue: number;
  requestField?: string | null;
}): FakeMediaCapabilityAxis {
  return {
    id: input.id,
    label: input.label,
    role: "param",
    control: {
      range: {
        min: input.min,
        max: input.max,
        step: input.step,
        default: input.defaultValue
      }
    },
    requestField: input.requestField === undefined ? input.id : input.requestField,
    wireType: "number"
  };
}

function imageCapability(input: {
  providerId: string;
  providerDisplayName: string;
  modelId: string;
  modelDisplayName: string;
  axes: FakeMediaCapabilityAxis[];
}): FakeMediaCapability {
  return {
    providerId: input.providerId,
    providerDisplayName: input.providerDisplayName,
    modelId: input.modelId,
    modelDisplayName: input.modelDisplayName,
    kind: "image",
    operation: "generate",
    adapter: "images_json",
    axes: input.axes,
    status: "available",
    source: "fake-daemon",
    reason: null,
    checkedAtMs: baseTime
  };
}

function videoCapability(input: {
  providerId: string;
  providerDisplayName: string;
  modelId: string;
  modelDisplayName: string;
  adapter: string;
  axes: FakeMediaCapabilityAxis[];
}): FakeMediaCapability {
  return {
    providerId: input.providerId,
    providerDisplayName: input.providerDisplayName,
    modelId: input.modelId,
    modelDisplayName: input.modelDisplayName,
    kind: "video",
    operation: "generate",
    adapter: input.adapter,
    axes: input.axes,
    status: "available",
    source: "fake-daemon",
    reason: null,
    checkedAtMs: baseTime
  };
}
```

- [ ] **Step 2: Convert fixture calls from `parameters` to `axes`**

For image fixtures, replace each `parameters: [` key with `axes: [` and each `mediaParameter({ name:` call with `mediaEnumAxis({ id:`. For video fixtures:

```ts
function configurableVideoCapability(): FakeMediaCapability {
  return videoCapability({
    providerId: "runway",
    providerDisplayName: "Runway",
    modelId: "gen-4",
    modelDisplayName: "Gen-4",
    adapter: "replicate_video",
    axes: [
      mediaEnumAxis({
        id: "aspect_ratio",
        label: "Aspect ratio",
        values: ["16:9", "9:16"],
        defaultValue: "16:9",
        requestField: "metadata.ratio"
      }),
      mediaRangeAxis({
        id: "duration_seconds",
        label: "Duration",
        min: 5,
        max: 12,
        step: 1,
        defaultValue: 8,
        requestField: "seconds"
      })
    ]
  });
}
```

Use `mediaEnumAxis` for finite provider choices such as `resolution`; use `mediaRangeAxis` only where the UI should render a bounded numeric input.

- [ ] **Step 3: Update saved settings fixtures and expected payloads**

Replace old saved video settings:

```ts
video: {
  providerId: "runway",
  logicalModelId: "gen-4",
  selections: {
    aspect_ratio: "16:9",
    duration_seconds: "8"
  }
}
```

Replace old expected update payloads with logical payloads. For the configurable video save test, the expected payload is:

```ts
expect(update.params).toEqual({
  media: {
    image: null,
    video: {
      providerId: "runway",
      logicalModelId: "gen-4",
      selections: {
        aspect_ratio: "9:16",
        duration_seconds: "12"
      }
    }
  }
});
```

For the provider-options video test, the expected payload is:

```ts
expect(update.params).toEqual({
  media: {
    image: null,
    video: {
      providerId: "byteplus",
      logicalModelId: "dreamina-seedance-2-0-260128",
      selections: {
        aspect_ratio: "1:1",
        duration_seconds: "12",
        resolution: "1080p"
      }
    }
  }
});
```

For the stale provider-options test, the expected payload is:

```ts
expect(update.params).toEqual({
  media: {
    image: null,
    video: {
      providerId: "byteplus",
      logicalModelId: "dreamina-seedance-2-0-260128",
      selections: {
        aspect_ratio: "16:9",
        duration_seconds: "5",
        resolution: "720p"
      }
    }
  }
});
```

For the image settings save test, the expected payload is:

```ts
expect(update.params).toEqual({
  media: {
    image: {
      providerId: "byteplus",
      logicalModelId: "seedream-3",
      selections: {
        size: "1280x720",
        quality: "standard",
        output_format: "png"
      }
    },
    video: null
  }
});
```

- [ ] **Step 4: Run focused Playwright tests and confirm they fail on UI implementation**

Run from `apps/puffer-desktop`:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer (image|video) generation settings"
```

Expected: FAIL because `MediaSettingsModal.svelte` still reads `capability.parameters`.

- [ ] **Step 5: Keep this red state uncommitted**

Do not commit after this task. `MediaSettingsModal.svelte` still reads `capability.parameters`. Task 5 commits the complete desktop migration after the modal is axis-native.

### Task 5: Rewrite Media Settings Modal Around Axes

**Files:**
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`

- [ ] **Step 1: Replace imports and state**

In `MediaSettingsModal.svelte`, remove `MediaCapabilityParameterInfo` from the type imports and add helper imports:

```ts
import {
  axisControlKind,
  axisDefaultValue,
  axisOptions,
  capabilityAxesError,
  normalizeAxisSelections,
  selectionIsValid
} from "./mediaAxisControls";
```

Replace the parameter-specific constants and state with:

```ts
const initialSaved = untrack(() => mediaSettingsForKind(kind, settings));
const initialSelections = untrack(() => initialSaved?.selections ?? {});

let providerId = $state(initialSaved?.providerId ?? "");
let logicalModelId = $state(initialSaved?.logicalModelId ?? "");
let selections = $state<Record<string, string>>({ ...initialSelections });
let appliedSettingsKey = $state(untrack(() => mediaSettingsKey(kind, settings)));
```

Replace derived fields that reference `selectedCapability.parameters`, aspect ratio, or duration with:

```ts
let selectedCapability = $derived(currentMatchingCapability());
let selectedCapabilityError = $derived(
  selectedCapability ? capabilityAxesError(selectedCapability.axes) : null
);
let mediaContentReady = $derived(settingsReady && !loading);
let hasAvailableCapabilities = $derived(availableCapabilities.length > 0);
let savedSelectionMissing = $derived(
  !loading &&
    savedSelectionIsConfigured(kind, settings) &&
    !savedSelectionIsAvailable(kind, settings, availableCapabilities)
);
let canSave = $derived(
  Boolean(settingsReady && selectedCapability && !selectedCapabilityError && !loading && !saving)
);
```

- [ ] **Step 2: Replace identity, apply, select, and save helpers**

Replace `mediaSettingsKey`, `applySettings`, `selectCapability`, `capabilityKey`, `currentCapabilityKey`, `currentMatchingCapability`, `capabilityMatchesIdentity`, `savedSelectionIsAvailable`, and `withCurrentSelection` with:

```ts
function mediaSettingsKey(mediaKind: MediaKind, mediaSettings: MediaSettings): string {
  const image = mediaSettings.image;
  const video = mediaSettings.video;
  return [
    mediaKind,
    image?.providerId ?? "",
    image?.logicalModelId ?? "",
    serializeSelections(image?.selections ?? {}),
    video?.providerId ?? "",
    video?.logicalModelId ?? "",
    serializeSelections(video?.selections ?? {})
  ].join("\u0000");
}

function applySettings(mediaSettings: MediaSettings) {
  const current = mediaSettingsForKind(kind, mediaSettings);
  providerId = current?.providerId ?? "";
  logicalModelId = current?.logicalModelId ?? "";
  selections = { ...(current?.selections ?? {}) };
}

function selectCapability(capability: MediaCapabilityInfo) {
  providerId = capability.providerId;
  logicalModelId = capability.modelId;
  selections = normalizeAxisSelections(capability.axes, selections);
}

function capabilityKey(capability: MediaCapabilityInfo): string {
  return [capability.providerId, capability.modelId].join("\u0000");
}

function currentCapabilityKey(): string {
  return logicalModelId ? [providerId, logicalModelId].join("\u0000") : "";
}

function currentMatchingCapability(): MediaCapabilityInfo | null {
  return (
    availableCapabilities.find((capability) =>
      capabilityMatchesIdentity(capability, kind, providerId, logicalModelId)
    ) ?? null
  );
}

function capabilityMatchesIdentity(
  capability: MediaCapabilityInfo,
  mediaKind: MediaKind,
  selectedProviderId: string | null | undefined,
  selectedLogicalModelId: string | null | undefined
): boolean {
  if (!selectedProviderId || !selectedLogicalModelId) return false;
  return (
    capability.kind === mediaKind &&
    capability.providerId === selectedProviderId &&
    capability.modelId === selectedLogicalModelId
  );
}

function savedSelectionIsAvailable(
  mediaKind: MediaKind,
  mediaSettings: MediaSettings,
  available: MediaCapabilityInfo[]
): boolean {
  const current = mediaSettingsForKind(mediaKind, mediaSettings);
  if (!current) return false;
  return available.some((capability) =>
    capabilityMatchesIdentity(
      capability,
      mediaKind,
      current.providerId,
      current.logicalModelId
    )
  );
}

function withCurrentSelection(): MediaGenerationSettings {
  const normalizedSelections = selectedCapability
    ? normalizeAxisSelections(selectedCapability.axes, selections)
    : selections;
  return {
    providerId,
    logicalModelId,
    selections: normalizedSelections
  };
}
```

- [ ] **Step 3: Replace axis value helpers**

Remove every helper that mentions `MediaCapabilityParameterInfo`, `parameter`, `aspectRatio`, or `durationSeconds`. Add:

```ts
function axisValue(axisId: string): string {
  return selections[axisId] ?? "";
}

function setAxisValue(axisId: string, value: string) {
  selections = { ...selections, [axisId]: value };
}

function axisReadOnlyValue(axisId: string): string {
  return selections[axisId] ?? "";
}

function rangeControl(axis: MediaCapabilityInfo["axes"][number]) {
  return "range" in axis.control ? axis.control.range : null;
}

function boolChecked(axisId: string): boolean {
  return selections[axisId] === "true";
}

function boolLabel(axisId: string): string {
  return boolChecked(axisId) ? "Enabled" : "Disabled";
}

function serializeSelections(value: Record<string, string>): string {
  return Object.entries(value)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, val]) => `${key}=${val}`)
    .join("\u0000");
}
```

- [ ] **Step 4: Replace normalization effects**

Replace the separate image/video `$effect` blocks that normalize parameters with one axis effect:

```ts
$effect(() => {
  if (!selectedCapability || selectedCapabilityError) return;
  const next = normalizeAxisSelections(selectedCapability.axes, selections);
  if (serializeSelections(next) !== serializeSelections(selections)) {
    selections = next;
  }
});
```

In `chooseDefaultCapability()`, replace the saved-capability branch with:

```ts
if (savedCapability) {
  selections = normalizeAxisSelections(savedCapability.axes, selections);
  return;
}
```

- [ ] **Step 5: Replace form markup for axes**

Inside the `.pf-media-form-grid`, keep the provider/model/folder markup, but replace the image/video parameter branches with one axis loop before the folder branch:

```svelte
{#if selectedCapabilityError}
  <p class="pf-media-state" data-warning="true" role="alert">{selectedCapabilityError}</p>
{:else if selectedCapability}
  {#each selectedCapability.axes as axis (axis.id)}
    {@const kind = axisControlKind(axis)}
    {#if kind === "enum" && axisOptions(axis).length > 1}
      <label class="pf-media-field">
        <span class="pf-field-label">{axis.label}</span>
        <select
          class="sc-input"
          value={axisValue(axis.id)}
          onchange={(event) => setAxisValue(axis.id, event.currentTarget.value)}
        >
          {#each axisOptions(axis) as option}
            <option value={option}>{option}</option>
          {/each}
        </select>
      </label>
    {:else if kind === "enum"}
      {@render readOnlyField(axis.label, axisReadOnlyValue(axis.id) || axisDefaultValue(axis) || "")}
    {:else if kind === "range" && rangeControl(axis)}
      {@const range = rangeControl(axis)}
      <label class="pf-media-field">
        <span class="pf-field-label">{axis.label}</span>
        <input
          class="sc-input"
          type="number"
          min={range.min}
          max={range.max}
          step={range.step || "any"}
          value={axisValue(axis.id)}
          onchange={(event) => {
            const value = event.currentTarget.value;
            setAxisValue(axis.id, selectionIsValid(axis, value) ? value : axisDefaultValue(axis) || "");
          }}
        />
      </label>
    {:else if kind === "bool"}
      <label class="pf-media-field pf-media-checkbox-field">
        <span class="pf-field-label">{axis.label}</span>
        <label class="pf-media-checkbox-row">
          <input
            type="checkbox"
            checked={boolChecked(axis.id)}
            onchange={(event) => setAxisValue(axis.id, event.currentTarget.checked ? "true" : "false")}
          />
          <span>{boolLabel(axis.id)}</span>
        </label>
      </label>
    {:else}
      <p class="pf-media-state" data-warning="true" role="alert">
        Capability axis {axis.id || "(missing id)"} is malformed.
      </p>
    {/if}
  {/each}
{/if}
```

Then keep the existing image folder block when `kind === "image"` and the existing video folder block when `kind === "video"`.

- [ ] **Step 6: Add minimal checkbox styling**

Append this CSS near the existing `.pf-media-field` styles:

```css
.pf-media-checkbox-field {
  gap: 8px;
}

.pf-media-checkbox-row {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 34px;
  font-size: 13px;
  color: var(--foreground);
}
```

- [ ] **Step 7: Run focused tests**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaAxisControls.test.ts src/lib/screens/agent/mediaCapabilityState.test.ts
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer (image|video) generation settings"
npm run check
```

Expected: PASS for all three commands.

- [ ] **Step 8: Prove the crash path is gone**

Run from repo root:

```bash
rg "capability\\.parameters|capability\\.defaults|MediaCapabilityParameterInfo|\\.parameters" apps/puffer-desktop/src/lib/screens/agent apps/puffer-desktop/src/lib/types.ts
```

Expected: no matches for `capability.parameters`, `capability.defaults`, or `MediaCapabilityParameterInfo` in the modal/types path. Matches in unrelated generated-media attachment code are acceptable only outside the files listed in this command.

- [ ] **Step 9: Commit the modal migration**

```bash
git add apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts
git commit -m "fix: render media settings from capability axes"
```

### Task 6: Strengthen CLI Daemon Capability Contract Tests

**Files:**
- Modify: `crates/puffer-cli/src/daemon.rs`

- [ ] **Step 1: Update the video capability test first**

In `daemon_list_media_capabilities_returns_video_capability()`, replace the final assertion with:

```rust
let capability = capabilities
    .iter()
    .find(|capability| capability["providerId"] == "replicate")
    .expect("replicate video capability");

assert_eq!(capability["adapter"], "replicate_video");
assert_eq!(capability["status"], "available");
assert!(capability.get("parameters").is_none());
assert!(capability.get("defaults").is_none());

let axes = capability["axes"].as_array().expect("axes");
assert!(axes.iter().any(|axis| {
    axis["id"] == "aspect_ratio"
        && axis["role"] == "param"
        && axis["requestField"] == "aspect_ratio"
        && axis["wireType"] == "string"
        && axis["control"]["enum"]["default"] == "16:9"
}));
assert!(axes.iter().any(|axis| {
    axis["id"] == "duration_seconds"
        && axis["requestField"] == "duration"
        && axis["control"]["enum"]["default"] == "5"
}));
```

In `daemon_list_media_capabilities_returns_relaydance_video_capability()`, add:

```rust
let capability = capabilities
    .iter()
    .find(|capability| capability["providerId"] == "relaydance")
    .expect("relaydance video capability");
assert!(capability.get("parameters").is_none());
assert!(capability.get("defaults").is_none());
assert!(capability["axes"].as_array().is_some_and(|axes| {
    axes.iter().any(|axis| {
        axis["id"] == "resolution"
            && axis["control"]["enum"]["default"] == "720p"
            && axis["requestField"] == "metadata.resolution"
    })
}));
```

- [ ] **Step 2: Run the focused Rust tests**

Run from repo root:

```bash
cargo test -p puffer-cli daemon_list_media_capabilities_returns_video_capability
cargo test -p puffer-cli daemon_list_media_capabilities_returns_relaydance_video_capability
```

Expected: PASS.

- [ ] **Step 3: Commit the daemon test guard**

```bash
git add crates/puffer-cli/src/daemon.rs
git commit -m "test: guard daemon media capability axis contract"
```

### Task 7: Final Validation And Cleanup

**Files:**
- Review only: `apps/puffer-desktop/src/lib/types.ts`
- Review only: `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
- Review only: `apps/puffer-desktop/tests/support/fakeDaemon.ts`
- Review only: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
- Review only: `crates/puffer-cli/src/daemon.rs`

- [ ] **Step 1: Run desktop focused validation**

Run from `apps/puffer-desktop`:

```bash
npm exec vitest -- run src/lib/screens/agent/mediaAxisControls.test.ts src/lib/screens/agent/mediaCapabilityState.test.ts
npm run check
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer (image|video) generation settings"
```

Expected: PASS.

- [ ] **Step 2: Run daemon focused validation**

Run from repo root:

```bash
cargo test -p puffer-cli daemon_list_media_capabilities_returns_video_capability
cargo test -p puffer-cli daemon_list_media_capabilities_returns_relaydance_video_capability
cargo test -p puffer-cli update_config_accepts_media_defaults
```

Expected: PASS.

- [ ] **Step 3: Scan for old settings/capability fields in changed desktop paths**

Run from repo root:

```bash
rg "parameters|defaults|modelId|adapter" apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts
```

Expected:
- No `capability.parameters` or `capability.defaults` in `MediaSettingsModal.svelte`.
- No persisted media `parameters` in `types.ts` or fake daemon settings normalization.
- `modelId` remains only as the daemon capability logical model id and generated media result field.
- `adapter` remains only as optional capability metadata or generated media/runtime metadata, not persisted settings.

- [ ] **Step 4: Review the diff**

Run from repo root:

```bash
git diff -- apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts crates/puffer-cli/src/daemon.rs
```

Expected: diff is limited to axis-native media settings and focused tests.

- [ ] **Step 5: Commit any final cleanup**

If Step 4 required small cleanup edits, commit them:

```bash
git add apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts crates/puffer-cli/src/daemon.rs
git commit -m "chore: finalize axis-native media settings cleanup"
```

If there are no cleanup edits, do not create an empty commit.
