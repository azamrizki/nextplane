import { ADMIN, expect, login, test, choose } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";
let bobLink = ""; // a workspace admin, so a project admin who may edit Ana's page

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
    const inv = await (await fetch("/api/v1/workspaces/acme/invites", { method: "POST", headers: h, body: JSON.stringify({ invites: [{ email: "bob@example.com", role: "admin" }] }) })).json();
    return { pid: p.id as string, link: inv[0].link as string };
  });
  projectUrl = `${hub.url}/acme/projects/${r.pid}`;
  bobLink = r.link;
});

test("two people write the same page live, see each other, and it is kept", async ({ page, browser, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await page.getByRole("link", { name: "Pages" }).click();
  await page.getByRole("button", { name: "New page" }).click();
  await page.getByLabel("Page title").fill("Launch plan");
  await page.getByLabel("Page title").press("Enter");
  const anaEditor = page.locator(".page-prose");
  await anaEditor.click();
  await page.keyboard.type("Ana writes first.");
  await expect(page.locator(".page-link", { hasText: "Launch plan" })).toBeVisible();
  const pageUrl = page.url();

  // Bob joins and opens the same page.
  const ctx = await browser.newContext();
  const bob = await ctx.newPage();
  await bob.goto(bobLink.replace(/^https?:\/\/[^/]+/, hub.url));
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByLabel("Password").fill("password123");
  await bob.getByRole("button", { name: "Create your account" }).click();
  await expect(bob).toHaveURL(/\/acme$/);
  await bob.goto(pageUrl);
  const bobEditor = bob.locator(".page-prose");
  await expect(bobEditor).toContainText("Ana writes first.");
  await expect(bob.getByRole("heading", { name: "Launch plan" }).or(bob.getByLabel("Page title"))).toBeVisible();

  // Both see two people here, and Bob's typing reaches Ana with his named caret.
  await expect(page.locator(".presence-dot")).toHaveCount(2, { timeout: 5000 });
  await bobEditor.click();
  await bob.keyboard.press("End");
  await bob.keyboard.type(" Bob adds a line.");
  await expect(anaEditor).toContainText("Bob adds a line.", { timeout: 5000 });
  await expect(page.locator(".collaboration-carets__label", { hasText: "Bob" })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/pages-live.png` });

  // Ana locks it: Bob's editor turns read-only.
  await page.getByRole("button", { name: "Lock", exact: true }).click();
  await expect(bob.getByText("Locked: nobody can edit")).toBeVisible({ timeout: 5000 });
  await expect(bobEditor).toHaveAttribute("contenteditable", "false");
  await page.getByRole("button", { name: "Unlock" }).click();
  await ctx.close();

  // Reload: the text was saved on the server.
  await page.waitForTimeout(2500);
  await page.reload();
  await expect(page.locator(".page-prose")).toContainText("Ana writes first. Bob adds a line.");

  // The server kept a version on the first save; save one more, then restore it.
  await page.getByRole("button", { name: "Versions" }).click();
  await expect(page.locator(".version-item")).toHaveCount(1, { timeout: 5000 });
  await page.getByRole("button", { name: "Save version" }).click();
  await expect(page.locator(".version-item")).toHaveCount(2);
  if (shots) await page.screenshot({ path: `${shots}/pages-versions.png` });
  await page.locator(".page-prose").click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type(" A mistake.");
  await expect(page.locator(".page-prose")).toContainText("A mistake.");
  await page.locator(".version-item").first().click();
  await page.getByRole("button", { name: "Restore this version" }).click();
  await expect(page.getByText("Restored.")).toBeVisible();
  await expect(page.locator(".page-prose")).not.toContainText("A mistake.");
  await expect(page.locator(".page-prose")).toContainText("Bob adds a line.");

  // The page shows up in search.
  const hits = await page.evaluate(async () => (await (await fetch("/api/v1/workspaces/acme/search?q=Bob")).json()).pages.length);
  expect(hits).toBe(1);
});

test("organize pages: nest by dragging, move, duplicate", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/pages");
  for (const title of ["Alpha", "Beta"]) {
    await page.getByRole("button", { name: "New page", exact: true }).click();
    await expect(page.getByLabel("Page title")).toHaveValue("");
    await page.getByLabel("Page title").fill(title);
    await page.getByLabel("Page title").press("Enter");
    await expect(page.locator(".page-link", { hasText: title })).toBeVisible();
  }
  // Drag Beta onto Alpha: it goes inside (indented under it).
  const beta = page.locator(".page-link", { hasText: /^Beta$/ });
  await beta.dragTo(page.locator(".page-link", { hasText: /^Alpha$/ }));
  await expect(page.locator('li[data-page="Beta"]')).toHaveAttribute("style", /padding-inline-start: 14px/);
  // Move it back to the top level with the dialog.
  await beta.click();
  await page.getByRole("button", { name: "Move", exact: true }).click();
  await choose(page.getByLabel("Put it inside"), { label: "Top level" });
  await page.getByRole("dialog").getByRole("button", { name: "Move" }).click();
  await expect(page.locator('li[data-page="Beta"]')).toHaveAttribute("style", /padding-inline-start: 0px/);
  // Duplicate.
  await page.getByRole("button", { name: "Duplicate" }).click();
  await expect(page.locator(".page-link", { hasText: "Beta (copy)" })).toBeVisible();
});

test("selected text becomes a work item with a live reference", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/pages");
  await page.getByRole("button", { name: "New page", exact: true }).click();
  await expect(page.getByLabel("Page title")).toHaveValue("");
  await page.getByLabel("Page title").fill("Bugs");
  await page.getByLabel("Page title").press("Enter");
  await page.locator(".page-prose").click();
  await page.keyboard.type("Fix the login button");
  await page.keyboard.press("Shift+Home");
  await page.getByRole("button", { name: "Create work item" }).click();
  const ref = page.locator(".wi-ref", { hasText: "WEB-1" });
  await expect(ref).toBeVisible();
  await expect(page.locator(".page-prose")).toContainText("WEB-1 Fix the login button");
  // Hover: a card with the item's title and state; open it.
  await ref.hover();
  const card = page.getByRole("tooltip");
  await expect(card).toContainText("Fix the login button");
  await expect(card).toContainText("Todo");
  if (shots) await page.screenshot({ path: `${shots}/pages-ref.png` });
  await card.getByRole("link", { name: "Open" }).click();
  await expect(page).toHaveURL(/\/work-items\//);
  await expect(page.getByText("Fix the login button").first()).toBeVisible();
});
