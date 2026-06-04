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
  file,
  previewUrl: "blob:preview"
};

const turnOptions: AgentTurnOptions = {
  attachmentIds: [attachment.id]
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
