import { ADMIN, expect, login, test } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";
let pid = "";
let guestLink = "";

test("setup: project with intake, a guest's invite", async ({ page, hub }) => {
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
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}`, { method: "PATCH", headers: h, body: JSON.stringify({ feature_intake: true }) });
    const inv = await (await fetch("/api/v1/workspaces/acme/invites", { method: "POST", headers: h, body: JSON.stringify({ invites: [{ email: "gina@example.com", role: "guest" }] }) })).json();
    return { pid: p.id as string, link: inv[0].link as string };
  });
  pid = r.pid;
  projectUrl = `${hub.url}/acme/projects/${r.pid}`;
  guestLink = r.link;
});

test("a guest asks, the admin accepts, the guest sees the outcome", async ({ page, browser, hub }) => {
  const ctx = await browser.newContext();
  const gina = await ctx.newPage();
  await gina.goto(guestLink.replace(/^https?:\/\/[^/]+/, hub.url));
  await gina.getByLabel("Your name").fill("Gina");
  await gina.getByLabel("Password").fill("password123");
  await gina.getByRole("button", { name: "Create your account" }).click();
  await expect(gina).toHaveURL(/\/acme$/);

  await login(page, hub);
  await page.goto(projectUrl);
  await page.evaluate(async (pid) => {
    const members = await (await fetch("/api/v1/workspaces/acme/members")).json();
    const g = members.find((m: { email: string }) => m.email === "gina@example.com");
    await fetch(`/api/v1/workspaces/acme/projects/${pid}/members`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ user_id: g.user_id, role: "guest" }) });
  }, pid);

  // Gina sends a request.
  await gina.goto(projectUrl + "/intake");
  await gina.getByRole("button", { name: "New request" }).click();
  await gina.getByLabel("Title").fill("Add CSV export");
  await gina.getByRole("dialog").getByRole("textbox", { name: "Description" }).fill("We need it for the monthly report.");
  await gina.getByRole("button", { name: "Send request" }).click();
  await expect(gina.locator(".intake-detail")).toContainText("Add CSV export");
  await expect(gina.locator(".intake-detail .intake-status")).toHaveText("Pending");

  // Ana sees the count, opens it and accepts.
  await page.goto(projectUrl + "/intake");
  await expect(page.locator(".tab-count")).toHaveText("1");
  await page.locator(".intake-row", { hasText: "Add CSV export" }).click();
  await expect(page.locator(".intake-detail")).toContainText("We need it for the monthly report.");
  if (shots) await page.screenshot({ path: `${shots}/intake.png` });
  await page.getByRole("button", { name: "Accept" }).click();
  await expect(page.locator(".intake-detail .intake-status")).toHaveText("Accepted");
  await page.getByRole("link", { name: "Accepted as WEB-1" }).click();
  await expect(page).toHaveURL(/\/work-items\//);
  await page.goto(projectUrl);
  await expect(page.locator(".list-row", { hasText: "Add CSV export" })).toBeVisible();

  // Gina sees what happened, in her closed requests.
  await gina.goto(projectUrl + "/intake");
  await gina.getByRole("tab", { name: "Closed" }).click();
  await expect(gina.locator(".intake-row", { hasText: "Add CSV export" })).toContainText("Accepted");
  await ctx.close();
});

test("people without an account use the public form", async ({ page, browser, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/intake");
  await page.getByRole("button", { name: "Public form" }).click();
  await page.getByRole("button", { name: "Turn on" }).click();
  const link = await page.getByRole("dialog").locator("input.input").inputValue();
  expect(link).toContain("/intake/");
  await page.keyboard.press("Escape");

  const ctx = await browser.newContext();
  const visitor = await ctx.newPage();
  await visitor.goto(link);
  await expect(visitor.getByText("Send a request to Website")).toBeVisible();
  await visitor.getByLabel("What do you need?").fill("Cannot log in on mobile");
  await visitor.getByLabel("Details").fill("The button does nothing on my phone.");
  await visitor.getByLabel("Email (optional, to hear back)").fill("rina@example.com");
  await visitor.getByRole("button", { name: "Send" }).click();
  await expect(visitor.getByText("Thank you! Your request was sent.")).toBeVisible();
  await ctx.close();

  await page.reload();
  await page.locator(".intake-row", { hasText: "Cannot log in on mobile" }).click();
  await expect(page.locator(".intake-detail")).toContainText("via the public form");
  await expect(page.locator(".intake-detail")).toContainText("rina@example.com");
  await page.getByRole("button", { name: "Decline" }).click();
  await page.getByLabel("Reason (shown to the person who asked)").fill("Fixed in the next app version");
  await page.getByRole("dialog").getByRole("button", { name: "Decline" }).click();
  await expect(page.locator(".intake-detail .intake-status")).toHaveText("Declined");
  await expect(page.locator(".intake-detail")).toContainText("Declined: Fixed in the next app version");
});
