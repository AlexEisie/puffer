import { describe, it, expect } from "vitest";
import { initialValues } from "./inlineCanvasInitialValues";

describe("initialValues new primitives", () => {
  it("collects textarea/editableTable/mediaPicker", () => {
    const v = initialValues({ body: [
      { type: "textarea", id: "script" },
      { type: "editableTable", id: "sb", rows: [["a","b"]] },
      { type: "mediaPicker", id: "one" },
      { type: "mediaPicker", id: "many", multi: true },
    ]});
    expect(v).toEqual({ script: "", sb: [["a","b"]], one: null, many: [] });
  });
  it("keeps existing primitives unchanged", () => {
    const v = initialValues({ body: [{ type: "toggle", id: "t" }] });
    expect(v).toEqual({ t: false });
  });
});
