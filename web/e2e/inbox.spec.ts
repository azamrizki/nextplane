import { ADMIN, expect, login, test } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";
let bobLink = "";

test("setup: project and Bob", async ({ page, hub }) => {
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
    const inv = await (await fetch("/api/v1/workspaces/acme/invites", { method: "POST", headers: h, body: JSON.stringify({ invites: [{ email: "bob@example.com", role: "member" }] }) })).json();
    return { pid: p.id as string, link: inv[0].link as string };
  });
  projectUrl = `${hub.url}/acme/projects/${r.pid}`;
  bobLink = r.link;
});

test("Bob is assigned and sees it live in his inbox", async ({ page, browser, hub }) => {
  const ctx = await browser.newContext();
  const bob = await ctx.newPage();
  await bob.goto(bobLink.replace(/^https?:\/\/[^/]+/, hub.url));
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByLabel("Password").fill("password123");
  await bob.getByRole("button", { name: "Create your account" }).click();
  await expect(bob).toHaveURL(/\/acme$/);
  await bob.goto(projectUrl);
  await bob.getByRole("button", { name: "Join" }).click();
  await bob.getByRole("link", { name: /Inbox/ }).click();
  await expect(bob.getByText("You're all caught up.")).toBeVisible();

  await login(page, hub);
  const bobId = await page.evaluate(async () => {
    const m = await (await fetch("/api/v1/workspaces/acme/members")).json();
    return m.find((x: { email: string }) => x.email === "bob@example.com").user_id as string;
  });
  await page.evaluate(async ([url, id]) => {
    const pid = (url ?? "").split("/").pop();
    await fetch(`/api/v1/workspaces/acme/projects/${pid}/work-items`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ title: "Review pricing page", assignee_ids: [id] }) });
  }, [projectUrl, bobId]);

  await expect(bob.locator(".inbox-row", { hasText: "Review pricing page" })).toBeVisible({ timeout: 5000 });
  await expect(bob.locator(".inbox-row")).toContainText("Ana assigned you");
  await expect(bob.locator(".side-count")).toHaveText("1");
  if (shots) await bob.screenshot({ path: `${shots}/inbox.png` });
  await bob.locator(".inbox-open").first().click();
  await expect(bob).toHaveURL(/\/work-items\//);
  await expect(bob.locator(".side-count")).toHaveCount(0);
  await ctx.close();
});

test("save as draft, find it in Drafts, publish", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await expect(page.locator(".list-row").first()).toBeVisible();
  await page.keyboard.press("c");
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Title").fill("Rough idea for onboarding");
  await dlg.getByRole("button", { name: "Save as draft" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".list-row", { hasText: "Rough idea" })).toHaveCount(0);
  await page.getByRole("link", { name: "Drafts" }).click();
  await expect(page.locator(".row", { hasText: "Rough idea for onboarding" })).toBeVisible();
  await page.locator(".row", { hasText: "Rough idea for onboarding" }).getByRole("button", { name: "Publish" }).click();
  await expect(page.getByText("No drafts.")).toBeVisible();
  await page.goto(projectUrl);
  await expect(page.locator(".list-row", { hasText: "Rough idea for onboarding" })).toBeVisible();
});
