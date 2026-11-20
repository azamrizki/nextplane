import { createHmac } from "node:crypto";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { ADMIN, expect, login, test } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";

test("setup: a project with work", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const pid = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Login is broken", description_html: "<p>Users see a blank page.</p>" }) });
    return p.id as string;
  });
  projectUrl = `${hub.url}/acme/projects/${pid}`;
});

test("import from CSV and export", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/settings/data");
  const csv = "Summary,Status,Priority,Notes\nWrite release notes,Todo,high,For 1.0\nFix dark mode,In Progress,low,\nUpdate docs,Done,,\n";
  await page.getByLabel("Choose a CSV file").setInputFiles({ name: "items.csv", mimeType: "text/csv", buffer: Buffer.from(csv) });
  await expect(page.getByText("3 rows found")).toBeVisible();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue("0");
  await expect(page.getByLabel("Description", { exact: true })).toHaveValue("3");
  await page.getByRole("button", { name: "Import 3 rows" }).click();
  await expect(page.getByText("3 work items created.")).toBeVisible();
  await page.goto(projectUrl);
  await expect(page.locator(".list-row", { hasText: "Fix dark mode" })).toBeVisible();

  await page.goto(projectUrl + "/settings/data");
  const href = await page.getByRole("link", { name: "Download CSV" }).getAttribute("href");
  const res = await page.request.get(hub.url + href!);
  expect(res.headers()["content-disposition"]).toContain("WEB-work-items.csv");
  const text = await res.text();
  expect(text).toContain("WEB-1,Login is broken");
  expect(text).toContain("WEB-3,Fix dark mode,In Progress");
});

test("a published board is readable without an account", async ({ page, browser, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/settings/publish");
  await page.getByRole("button", { name: "Publish" }).click();
  const link = await page.getByRole("textbox", { name: "Public board" }).inputValue();
  expect(link).toContain("/p/");

  const ctx = await browser.newContext();
  const visitor = await ctx.newPage();
  await visitor.goto(link);
  await expect(visitor.getByRole("heading", { name: "Website" })).toBeVisible();
  await expect(visitor.getByRole("region", { name: "In Progress" })).toContainText("Fix dark mode");
  if (shots) await visitor.screenshot({ path: `${shots}/public-board.png` });
  await visitor.getByRole("button", { name: /Login is broken/ }).click();
  await expect(visitor.getByRole("dialog")).toContainText("Users see a blank page.");
  await visitor.keyboard.press("Escape");
  await visitor.getByRole("tab", { name: "List" }).click();
  await visitor.getByRole("searchbox", { name: "Search" }).fill("docs");
  await expect(visitor.locator(".pb-card")).toHaveCount(1);

  await page.getByRole("button", { name: "Stop publishing" }).click();
  await visitor.reload();
  await expect(visitor.getByText("This board does not exist or is no longer public.")).toBeVisible();
  await ctx.close();
});

test("webhooks: add one, send a test, see it delivered and signed", async ({ page, hub }) => {
  const got: { headers: Record<string, string | string[] | undefined>; body: string }[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      got.push({ headers: req.headers, body });
      res.statusCode = 204;
      res.end();
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const port = (server.address() as AddressInfo).port;
  try {
    await login(page, hub);
    await page.goto(hub.url + "/acme/settings");
    await page.getByRole("link", { name: /Webhooks/ }).click();
    await page.getByLabel("Payload URL").fill(`http://127.0.0.1:${port}/hook`);
    await page.getByRole("button", { name: "Add webhook" }).click();
    const secret = await page.getByRole("textbox", { name: "Secret" }).inputValue();
    expect(secret).toMatch(/^whsec_/);
    await page.getByRole("button", { name: "Send test" }).click();
    await expect(page.locator(".hook-st", { hasText: "Delivered" })).toBeVisible({ timeout: 10_000 });
    expect(got.length).toBeGreaterThan(0);
    const ping = got.find((g) => g.headers["x-nextplane-event"] === "ping")!;
    const want = "sha256=" + createHmac("sha256", secret).update(ping.body).digest("hex");
    expect(ping.headers["x-nextplane-signature"]).toBe(want);
    // A change in the workspace arrives too.
    await page.evaluate(async (url) => {
      const pid = url.split("/projects/")[1];
      await fetch(`/api/v1/workspaces/acme/projects/${pid}/work-items`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ title: "From the API" }) });
    }, projectUrl);
    await expect.poll(() => got.some((g) => g.headers["x-nextplane-event"] === "work_item.created" && g.body.includes("From the API")), { timeout: 10_000 }).toBe(true);
  } finally {
    server.close();
  }
});

test("GitHub: a merged pull request links and completes the work item", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(hub.url + "/acme/settings/github");
  await page.getByRole("button", { name: "Connect GitHub" }).click();
  const url = await page.getByRole("textbox", { name: "Payload URL" }).inputValue();
  const secret = await page.getByRole("textbox", { name: "Secret" }).inputValue();
  const body = JSON.stringify({
    action: "closed",
    pull_request: { number: 3, title: "Fix the login page", body: "Fixes WEB-1", html_url: "https://github.com/acme/site/pull/3", state: "closed", merged: true, head: { ref: "fix-login" } },
  });
  const res = await page.request.post(url, {
    headers: { "content-type": "application/json", "x-github-event": "pull_request", "x-hub-signature-256": "sha256=" + createHmac("sha256", secret).update(body).digest("hex") },
    data: body,
  });
  expect(await res.json()).toEqual({ linked: 1, closed: 1 });
  await page.reload();
  await expect(page.getByText(/Last event/)).toBeVisible();
  await page.goto(projectUrl);
  await page.locator(".list-row", { hasText: "Login is broken" }).click();
  await expect(page.getByRole("dialog").getByRole("link", { name: /PR #3: Fix the login page \(merged\)/ })).toBeVisible();
});
