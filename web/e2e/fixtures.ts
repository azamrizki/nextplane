import { test as base, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const BIN = process.env.NEXTPLANE_E2E_BIN ?? resolve(fileURLToPath(new URL(".", import.meta.url)), "../../bin/nextplane");

async function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const s = createServer();
    s.once("error", fail);
    s.listen(0, "127.0.0.1", () => {
      const port = (s.address() as { port: number }).port;
      s.close(() => ok(port));
    });
  });
}

export interface Hub {
  url: string;
  setupLink: string;
  dataDir: string;
}

async function startHub(): Promise<{ hub: Hub; proc: ChildProcess }> {
  const dataDir = mkdtempSync(join(tmpdir(), "nextplane-e2e-"));
  const port = await freePort();
  const proc = spawn(BIN, ["--data", dataDir, "serve", "--listen", `127.0.0.1:${port}`], {
    env: { ...process.env, NEXTPLANE_LOG: "warn" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const setupLink = await new Promise<string>((ok, fail) => {
    let buf = "";
    const timer = setTimeout(() => fail(new Error("no setup link within 10 s:\n" + buf)), 10_000);
    proc.stderr!.on("data", (d: Buffer) => {
      buf += d.toString();
      const m = buf.match(/http:\/\/\S+\/setup\?token=[\w-]+/);
      if (m) {
        clearTimeout(timer);
        ok(m[0]);
      }
    });
    proc.once("exit", (c) => fail(new Error(`server exited ${c}:\n${buf}`)));
  });
  return { hub: { url: `http://127.0.0.1:${port}`, setupLink, dataDir }, proc };
}

/** `hub` is a fresh server per spec file (tests in one file share it, in order); data folders
 * are our own temp dirs, removed when the worker ends. */
export const test = base.extend<{ hub: Hub }, { hubs: Map<string, { hub: Hub; proc: ChildProcess }> }>({
  hubs: [
    async ({}, use) => {
      const hubs = new Map<string, { hub: Hub; proc: ChildProcess }>();
      await use(hubs);
      for (const { hub, proc } of hubs.values()) {
        proc.kill("SIGTERM");
        await new Promise((r) => proc.once("exit", r));
        if (hub.dataDir.startsWith(join(tmpdir(), "nextplane-e2e-"))) {
          rmSync(hub.dataDir, { recursive: true, force: true });
        }
      }
    },
    { scope: "worker" },
  ],
  hub: async ({ hubs }, use, testInfo) => {
    let h = hubs.get(testInfo.file);
    if (!h) {
      h = await startHub();
      hubs.set(testInfo.file, h);
    }
    await use(h.hub);
  },
});

export { expect };

export const ADMIN = { email: "ana@example.com", password: "correct horse", name: "Ana" };

/** Logs in through the form and waits until the app has left the login page. */
export async function login(page: import("@playwright/test").Page, hub: Hub, email = ADMIN.email, password = ADMIN.password) {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).not.toHaveURL(/\/login$/);
}

/** Opens the picker of a filter (Plane-style filter row): an applied one, or "Add filter". */
export async function openFilter(page: import("@playwright/test").Page, prop: string) {
  const applied = page.getByRole("button", { name: `Filter: ${prop}` });
  if (await applied.count()) {
    await applied.click();
    return;
  }
  if (!(await page.locator(".filter-row").count())) await page.getByRole("button", { name: /^Filters/ }).click();
  await page.getByRole("button", { name: "Add filter" }).click();
  await page.getByRole("menuitem", { name: prop, exact: true }).click();
}

/** Picks a value in one of our Select controls (a button that opens a listbox), as
 * `selectOption` does for a native select. `value` is the option's value, or `{ label }`. */
export async function choose(target: import("@playwright/test").Locator, value: string | { label: string }) {
  await target.click();
  const page = target.page();
  const list = page.locator('[role="listbox"]').last();
  const opt = typeof value === "string" ? list.locator(`[role="option"][data-value="${value}"]`) : list.getByRole("option", { name: value.label, exact: true });
  await opt.click();
}

/** Sets a date through our date picker: opens it from `trigger`, types the date, Enter. */
export async function pickDate(trigger: import("@playwright/test").Locator, iso: string) {
  await trigger.click();
  const box = trigger.page().getByRole("textbox", { name: "Type a date" }).last();
  await box.fill(iso);
  await box.press("Enter");
}

/** Sets a start and an end through the range picker opened from `trigger`, then applies. */
export async function pickRange(trigger: import("@playwright/test").Locator, start: string, end: string) {
  await trigger.click();
  const page = trigger.page();
  for (const [name, iso] of [["Start", start], ["End", end]] as const) {
    const box = page.getByRole("textbox", { name, exact: true }).last();
    await box.fill(iso);
    await box.press("Enter");
  }
  await page.getByRole("button", { name: "Apply" }).last().click();
}
