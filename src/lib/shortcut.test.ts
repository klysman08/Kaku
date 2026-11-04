import { describe, expect, it } from "vitest";
import { displayParts, findDuplicates, keyToken, normalize, recordKey } from "./shortcut";

const key = (code: string, mods: Partial<{ ctrl: boolean; shift: boolean; alt: boolean; meta: boolean }> = {}) => ({
  code,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
  metaKey: !!mods.meta,
});

describe("keyToken", () => {
  it("maps letters, digits and named keys", () => {
    expect(keyToken("KeyK")).toBe("K");
    expect(keyToken("Digit1")).toBe("1");
    expect(keyToken("Space")).toBe("Space");
    expect(keyToken("F12")).toBe("F12");
    expect(keyToken("ShiftLeft")).toBeNull();
    expect(keyToken("IntlRo")).toBeNull();
  });
});

describe("recordKey", () => {
  it("builds combos in a stable modifier order", () => {
    expect(recordKey(key("KeyK", { shift: true, ctrl: true }))).toEqual({ kind: "combo", value: "Ctrl+Shift+K" });
    expect(recordKey(key("Space", { alt: true, meta: true }))).toEqual({ kind: "combo", value: "Alt+Super+Space" });
  });

  it("waits while only modifiers are held", () => {
    expect(recordKey(key("ControlLeft", { ctrl: true }))).toEqual({ kind: "pending" });
  });

  it("requires a real modifier except for function keys", () => {
    expect(recordKey(key("KeyA"))).toEqual({ kind: "needsModifier" });
    expect(recordKey(key("KeyA", { shift: true }))).toEqual({ kind: "needsModifier" });
    expect(recordKey(key("F9"))).toEqual({ kind: "combo", value: "F9" });
  });

  it("handles cancel and clear", () => {
    expect(recordKey(key("Escape"))).toEqual({ kind: "cancel" });
    expect(recordKey(key("Backspace"))).toEqual({ kind: "clear" });
  });
});

describe("duplicates", () => {
  it("normalises modifier order and aliases", () => {
    expect(normalize("Shift+Ctrl+K")).toBe(normalize("Control+Shift+k"));
  });

  it("flags every command sharing a combo", () => {
    expect(findDuplicates({ popup: "Ctrl+Shift+1", review: "Shift+Ctrl+1", translate: "Ctrl+Shift+2", x: "" })).toEqual(
      new Set(["popup", "review"]),
    );
  });

  it("formats Super per platform", () => {
    expect(displayParts("Super+Shift+T")).toEqual(["Win", "Shift", "T"]);
    expect(displayParts("")).toEqual([]);
  });
});
