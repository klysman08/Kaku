import { describe, expect, it } from "vitest";
import { appendText } from "./useDictation";

describe("appendText", () => {
  it("starts an empty box with the dictated text", () => {
    expect(appendText("", "  Olá mundo ")).toBe("Olá mundo");
  });

  it("adds a space between pieces unless one is already there", () => {
    expect(appendText("Primeira frase.", "Segunda")).toBe("Primeira frase. Segunda");
    expect(appendText("Linha\n", "nova")).toBe("Linha\nnova");
    expect(appendText("fim ", "ok")).toBe("fim ok");
  });

  it("ignores empty results", () => {
    expect(appendText("texto", "   ")).toBe("texto");
  });
});
