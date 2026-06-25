// Pure rendering-decision logic for the Canvas `mediaPicker` node. Kept out of
// InlineCanvasNode.svelte so the image-vs-video branching, video access-URL
// dedup, and preview wiring are unit-testable without mounting a component.

export type MediaItemKind = "image" | "video";

export type MediaPickerItem = {
  id?: unknown;
  label?: unknown;
  description?: unknown;
  kind?: unknown;
  url?: unknown;
  path?: unknown;
};

/** Resolved video access URLs keyed by item id. A present key means the item has
 *  been resolved already; an empty-string value is the failure sentinel (the
 *  access RPC returned non-available or threw) so the id is never re-resolved. */
export type ResolvedVideoUrls = Readonly<Record<string, string>>;

/** Explicit per-item discrimination — never URL/extension sniffing. Anything
 *  that is not exactly `"video"` is treated as an image (the default). */
export function mediaItemKind(item: MediaPickerItem): MediaItemKind {
  return item.kind === "video" ? "video" : "image";
}

export function mediaItemId(item: MediaPickerItem): string {
  return typeof item.id === "string" ? item.id : "";
}

function asString(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** Video items (with a usable id + path) whose access URL has not yet been
 *  resolved, so the caller mints each one exactly once. */
export function videoItemsToResolve(
  items: MediaPickerItem[],
  resolved: ResolvedVideoUrls
): { id: string; path: string }[] {
  const out: { id: string; path: string }[] = [];
  for (const item of items) {
    if (mediaItemKind(item) !== "video") continue;
    const id = mediaItemId(item);
    const path = asString(item.path);
    if (!id || !path) continue;
    if (Object.prototype.hasOwnProperty.call(resolved, id)) continue;
    out.push({ id, path });
  }
  return out;
}

export type MediaThumb =
  | { kind: "image"; url: string; available: true }
  | { kind: "video"; url: string; available: boolean };

/** Thumbnail render state for one item. Images always render (matching the
 *  prior behaviour); a video is only available once a non-empty access URL is
 *  resolved, otherwise the cell shows a disabled "preview unavailable" state. */
export function mediaThumb(item: MediaPickerItem, resolved: ResolvedVideoUrls): MediaThumb {
  if (mediaItemKind(item) === "video") {
    const url = asString(resolved[mediaItemId(item)]);
    return { kind: "video", url, available: url.length > 0 };
  }
  return { kind: "image", url: asString(item.url), available: true };
}

export type MediaPreview = { kind: MediaItemKind; url: string };

/** Props for the click-to-open `CanvasMediaPreview` popup: an image's url is
 *  passed straight through; a video uses its resolved access URL (empty when
 *  unresolved/failed, in which case the popup simply has nothing to play). */
export function mediaPreview(item: MediaPickerItem, resolved: ResolvedVideoUrls): MediaPreview {
  if (mediaItemKind(item) === "video") {
    return { kind: "video", url: asString(resolved[mediaItemId(item)]) };
  }
  return { kind: "image", url: asString(item.url) };
}
