import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter } from "react-router";
import { RouterProvider } from "react-router/dom";
import { describe, expect, it, vi } from "vitest";
import { makeQueryClient, routes } from "../App";
import { I18nProvider } from "../i18n";

type Handler = (url: string, init?: RequestInit) => [number, unknown];

function serve(handler: Handler) {
  globalThis.fetch = vi.fn(async (url: string, init?: RequestInit) => {
    const [status, body] = handler(url, init);
    return new Response(body === undefined ? null : JSON.stringify(body), { status });
  }) as unknown as typeof fetch;
}

function renderAt(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <I18nProvider initial="en">
      <QueryClientProvider client={makeQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </I18nProvider>,
  );
  return router;
}

const user = { id: "1", email: "ana@example.com", name: "Ana", avatar: null, is_instance_admin: true, is_bot: false, created_at: "" };

describe("auth flow", () => {
  it("sends anonymous visitors to login, then home after logging in", async () => {
    let loggedIn = false;
    serve((url, init) => {
      if (url.endsWith("/instance")) return [200, { name: "Acme", setup_required: false }];
      if (url.endsWith("/me")) return loggedIn ? [200, user] : [401, { error: { code: "unauthorized", message: "log in first" } }];
      if (url.endsWith("/workspaces")) return [200, []];
      if (url.endsWith("/auth/login")) {
        const body = JSON.parse(String(init?.body));
        if (body.password !== "correct horse") return [401, { error: { code: "unauthorized", message: "wrong email or password" } }];
        loggedIn = true;
        return [200, user];
      }
      return [404, { error: { code: "not_found", message: "" } }];
    });
    const router = renderAt("/");
    expect(await screen.findByRole("heading", { name: "Log in" })).toBeTruthy();
    expect(router.state.location.pathname).toBe("/login");
    expect(screen.getByText("to Acme")).toBeTruthy();

    const u = userEvent.setup();
    await u.type(screen.getByLabelText("Email"), "ana@example.com");
    await u.type(screen.getByLabelText("Password"), "wrong");
    await u.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByText("Wrong email or password.")).toBeTruthy();

    await u.clear(screen.getByLabelText("Password"));
    await u.type(screen.getByLabelText("Password"), "correct horse");
    await u.click(screen.getByRole("button", { name: "Log in" }));
    expect(await screen.findByRole("heading", { name: "You are not in any workspace yet" })).toBeTruthy();
    expect(router.state.location.pathname).toBe("/create-workspace");
  });

  it("sends everyone to setup while setup is required", async () => {
    serve((url) => {
      if (url.endsWith("/instance")) return [200, { name: "", setup_required: true }];
      if (url.endsWith("/me")) return [401, { error: { code: "unauthorized", message: "" } }];
      if (url.endsWith("/setup/verify")) return [403, { error: { code: "forbidden", message: "" } }];
      return [404, {}];
    });
    const router = renderAt("/login");
    expect(await screen.findByRole("heading", { name: "This setup link does not work" })).toBeTruthy();
    expect(router.state.location.pathname).toBe("/setup");
  });
});
