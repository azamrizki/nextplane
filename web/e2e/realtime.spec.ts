import { ADMIN, expect, login, test } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";
let bobLink = "";

test("setup: workspace, project, Bob's invite", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const r = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Payment page crashes on Safari" }) });
    const inv = await (await fetch("/api/v1/workspaces/acme/invites", { method: "POST", headers: h, body: JSON.stringify({ invites: [{ email: "bob@example.com", role: "member" }] }) })).json();
    return { pid: p.id as string, link: inv[0].link as string };
  });
  projectUrl = `${hub.url}/acme/projects/${r.pid}`;
  bobLink = r.link;
});

test("a change by Bob appears for Ana without reloading", async ({ page, browser, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await expect(page.locator(".list-row")).toHaveCount(1);

  const ctx = await browser.newContext();
  const bob = await ctx.newPage();
  await bob.goto(bobLink.replace(/^https?:\/\/[^/]+/, hub.url));
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByLabel("Password").fill("password123");
  await bob.getByRole("button", { name: "Create your account" }).click();
  await expect(bob).toHaveURL(/\/acme$/);
  await bob.goto(projectUrl);
  await bob.getByRole("button", { name: "Join" }).click();
  await bob.locator('.list-add[data-group="Todo"]').click();
  await bob.locator(".list-row-form").getByLabel("Title").fill("Added by Bob");
  await bob.locator(".list-row-form").getByLabel("Title").press("Enter");

  // Ana's list updates through the WebSocket.
  await expect(page.locator(".list-row", { hasText: "Added by Bob" })).toBeVisible({ timeout: 5000 });
  // And a property change too.
  await bob.locator(".list-row", { hasText: "Payment page" }).click();
  await bob.getByRole("dialog").getByRole("button", { name: "Priority" }).click();
  await bob.getByRole("option", { name: "Urgent" }).click();
  await expect(page.locator(".list-row", { hasText: "Payment page" }).locator('[aria-label="Priority"] svg rect[fill="var(--prio-urgent)"]')).toHaveCount(1, { timeout: 5000 });
  await ctx.close();
});

test("command palette searches and jumps", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await expect(page.locator(".list-row").first()).toBeVisible();
  await page.keyboard.press("Control+k");
  const pal = page.getByRole("dialog");
  await pal.getByRole("combobox").fill("safari");
  await expect(pal.getByRole("option", { name: /Payment page crashes on Safari/ })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/palette.png` });
  await page.keyboard.press("ArrowDown");
  await pal.getByRole("option", { name: /Payment page crashes on Safari/ }).click();
  await expect(page).toHaveURL(/\/work-items\//);
  await expect(page.getByLabel("Title")).toHaveValue("Payment page crashes on Safari");

  await page.keyboard.press("Control+k");
  await page.getByRole("dialog").getByRole("combobox").fill("new work");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog").getByLabel("Title")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
});

test("create an API token and a bot", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(hub.url + "/profile");
  await page.getByRole("button", { name: "New token" }).click();
  await page.getByRole("dialog").getByLabel("Token name").fill("CI");
  await page.getByRole("dialog").getByRole("button", { name: "New token" }).click();
  const secret = await page.getByRole("dialog").locator("code").textContent();
  expect(secret).toMatch(/^np_/);
  const ok = await page.evaluate(async (tok) => (await fetch("/api/v1/me", { headers: { Authorization: `Bearer ${tok}` }, credentials: "omit" })).status, secret);
  expect(ok).toBe(200);

  await page.goto(hub.url + "/acme/settings");
  await page.getByRole("button", { name: "New bot" }).click();
  await page.getByRole("dialog").getByLabel("Bot name").fill("NextBoard agent");
  await page.getByRole("dialog").getByRole("button", { name: "New bot" }).click();
  await expect(page.getByRole("dialog").locator("code")).toContainText("np_");
  await page.keyboard.press("Escape");
  await expect(page.locator(".row", { hasText: "NextBoard agent" }).getByText("bot", { exact: true })).toBeVisible();
});
