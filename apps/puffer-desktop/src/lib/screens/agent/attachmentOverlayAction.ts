import type { MessageAttachment } from "../../types";

export type AttachmentOverlayAction =
  | { kind: "open_folder"; path: string }
  | { kind: "download"; url: string; suggestedName: string };

export function attachmentOverlayAction(
  attachment: MessageAttachment | null
): AttachmentOverlayAction | null {
  if (!attachment) return null;

  switch (attachment.source.kind) {
    case "local_file":
      return attachment.source.path
        ? { kind: "open_folder", path: attachment.source.path }
        : null;
    case "generated_media":
      return attachment.source.localPath
        ? { kind: "open_folder", path: attachment.source.localPath }
        : null;
    case "remote_url":
      return attachment.kind === "image" && attachment.source.url
        ? { kind: "download", url: attachment.source.url, suggestedName: attachment.name }
        : null;
  }
}
