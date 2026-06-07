import { expect, test } from "vitest";
import type { AgentTurnOptions } from "./desktop";
import type { AttachmentPreviewResult, MessageAttachment } from "../types";

const file = new File(["pixel"], "pixel.png", { type: "image/png" });

const attachment: MessageAttachment = {
  id: "attachment-1",
  name: "pixel.png",
  mimeType: "image/png",
  size: 5,
  extension: "PNG",
  kind: "image",
  state: "available",
  source: { kind: "user_upload" },
  file,
  previewUrl: "blob:preview"
};

const turnOptions: AgentTurnOptions = {
  attachmentIds: [attachment.id]
};

const generatedAttachment: MessageAttachment = {
  id: "generated-image:artifact-1",
  name: "Generated image",
  mimeType: "image/png",
  size: 8,
  extension: "PNG",
  kind: "image",
  state: "available",
  source: {
    kind: "generated_media",
    jobId: "job-1",
    artifactId: "artifact-1",
    index: 0
  }
};

const legacyTurnOptions: AgentTurnOptions = {
  // @ts-expect-error daemon-facing turn options no longer accept attachment metadata.
  attachments: [attachment]
};

const preview: AttachmentPreviewResult = {
  state: "available",
  mimeType: "image/png",
  bytes: [1, 2, 3]
};

void turnOptions;
void legacyTurnOptions;
void preview;

test("message attachments support generated media preview sources", () => {
  expect(generatedAttachment.source.kind).toBe("generated_media");
});

test("keeps generated media grouping fields", () => {
  expect(generatedAttachment.source.kind).toBe("generated_media");
  if (generatedAttachment.source.kind === "generated_media") {
    expect(generatedAttachment.source.jobId).toBe("job-1");
    expect(generatedAttachment.source.index).toBe(0);
  }
});
