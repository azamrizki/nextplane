import { describe, expect, it } from "vitest";
import { en } from "./en";
import { id } from "./id";
import { format } from "./index";

describe("i18n", () => {
  it("Indonesian has exactly the English keys", () => {
    expect(Object.keys(id).sort()).toEqual(Object.keys(en).sort());
  });

  it("placeholders match between languages", () => {
    const ph = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join(",");
    for (const k of Object.keys(en) as (keyof typeof en)[]) {
      expect(ph(id[k]), k).toBe(ph(en[k]));
    }
  });

  it("formats placeholders", () => {
    expect(format("Hi {name}", { name: "Ana" })).toBe("Hi Ana");
    expect(format("Hi {x}", {})).toBe("Hi {x}");
  });
});
