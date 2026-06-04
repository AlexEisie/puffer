import type { MessageAttachment } from "./types";

export type ChatOpenIntent =
  | { kind: "file"; path: string; line: number | null }
  | { kind: "attachment"; attachment: MessageAttachment };

export function fileOpenIntent(path: string, line: number | null = null): ChatOpenIntent {
  return { kind: "file", path, line };
}

export function attachmentOpenIntent(attachment: MessageAttachment): ChatOpenIntent {
  return { kind: "attachment", attachment };
}
