import { describe, it, expect } from "vitest";
import {
  mediaItemKind,
  mediaItemId,
  mediaItemArtifactId,
  videoItemsToResolve,
  mediaItemView,
} from "./mediaPickerItems";

describe("mediaItemKind", () => {
  it("returns video only for an explicit kind:'video'", () => {
    expect(mediaItemKind({ kind: "video" })).toBe("video");
  });
  it("defaults to image for image/absent/garbage kinds (no URL sniffing)", () => {
    expect(mediaItemKind({ kind: "image" })).toBe("image");
    expect(mediaItemKind({})).toBe("image");
    expect(mediaItemKind({ kind: 7 })).toBe("image");
    expect(mediaItemKind({ kind: "VIDEO" })).toBe("image");
  });
});

describe("mediaItemId", () => {
  it("reads a string id, else empty", () => {
    expect(mediaItemId({ id: "shot-001" })).toBe("shot-001");
    expect(mediaItemId({ id: 3 })).toBe("");
    expect(mediaItemId({})).toBe("");
  });
});

describe("mediaItemArtifactId", () => {
  it("reads a string artifactId, else empty", () => {
    expect(mediaItemArtifactId({ artifactId: "art-001" })).toBe("art-001");
    expect(mediaItemArtifactId({ artifactId: 3 })).toBe("");
    expect(mediaItemArtifactId({})).toBe("");
  });
});

describe("videoItemsToResolve", () => {
  it("returns each video item's artifactId exactly once, skipping images", () => {
    const items = [
      { id: "a", kind: "video", artifactId: "art-a" },
      { id: "b", kind: "image", url: "http://img/b.png" },
      { id: "c", kind: "video", artifactId: "art-c" },
    ];
    expect(videoItemsToResolve(items, {})).toEqual(["art-a", "art-c"]);
  });
  it("skips video items missing a usable artifactId or id", () => {
    const items = [
      { id: "a", kind: "video" },
      { id: "", kind: "video", artifactId: "art-a" },
      { kind: "video", artifactId: "art-b" },
    ];
    expect(videoItemsToResolve(items, {})).toEqual([]);
  });
  it("does not re-resolve an artifactId already present (success or failure sentinel)", () => {
    const items = [
      { id: "a", kind: "video", artifactId: "art-a" },
      { id: "b", kind: "video", artifactId: "art-b" },
    ];
    // art-a resolved to a poster url, art-b resolved to the failure sentinel ""
    // — both are done.
    expect(videoItemsToResolve(items, { "art-a": "blob:poster-a", "art-b": "" })).toEqual([]);
  });
});

describe("mediaItemView", () => {
  it("image item renders its url and is always available", () => {
    expect(mediaItemView({ kind: "image", url: "http://img/b.png" }, {})).toEqual({
      kind: "image",
      url: "http://img/b.png",
      available: true,
    });
  });
  it("video item with a resolved poster url is available", () => {
    expect(
      mediaItemView({ id: "a", kind: "video", artifactId: "art-a" }, { "art-a": "blob:poster-a" })
    ).toEqual({ kind: "video", url: "blob:poster-a", available: true });
  });
  it("video item whose poster is unresolved or failed is unavailable with no url", () => {
    const item = { id: "a", kind: "video", artifactId: "art-a" };
    expect(mediaItemView(item, {})).toEqual({ kind: "video", url: "", available: false });
    expect(mediaItemView(item, { "art-a": "" })).toEqual({
      kind: "video",
      url: "",
      available: false,
    });
  });
});
