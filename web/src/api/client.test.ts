import { describe, expect, it, vi } from "vitest";
import { ApiError, get, post } from "./client";

function mockFetch(status: number, body: string) {
  return vi.fn(async () => new Response(body || null, { status }));
}

describe("api client", () => {
  it("returns parsed JSON", async () => {
    globalThis.fetch = mockFetch(200, '{"name":"Acme"}') as typeof fetch;
    await expect(get<{ name: string }>("/instance")).resolves.toEqual({ name: "Acme" });
  });

  it("throws ApiError with the server's code and message", async () => {
    globalThis.fetch = mockFetch(401, '{"error":{"code":"unauthorized","message":"log in first"}}') as typeof fetch;
    const err = (await get("/me").catch((e: unknown) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(401);
    expect(err.code).toBe("unauthorized");
    expect(err.message).toBe("log in first");
  });

  it("handles 204 and non-JSON errors", async () => {
    globalThis.fetch = mockFetch(204, "") as typeof fetch;
    await expect(post("/auth/logout")).resolves.toBeUndefined();
    globalThis.fetch = mockFetch(502, "<html>bad gateway</html>") as typeof fetch;
    const err = (await get("/x").catch((e: unknown) => e)) as ApiError;
    expect(err.code).toBe("http_502");
  });

  it("sends JSON bodies to /api/v1", async () => {
    const f = mockFetch(200, "{}");
    globalThis.fetch = f as typeof fetch;
    await post("/auth/login", { email: "a@b.co" });
    const [url, init] = f.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/v1/auth/login");
    expect(init.method).toBe("POST");
    expect(init.body).toBe('{"email":"a@b.co"}');
  });
});
