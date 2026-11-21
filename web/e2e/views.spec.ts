import { ADMIN, expect, login, openFilter, test } from "./fixtures";

test.describe.configure({ mode: "serial" });

test("workspace views list work items across projects, filter them and save custom views", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const me = await (await fetch("/api/v1/me")).json();
    for (const [name, title, extra] of [
      ["Website", "Fix the footer", { priority: "urgent" }],
      ["Mobile", "Ship the beta", { assignee_ids: [me.id] }],
    ] as const) {
      const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name }) })).json();
      await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title, ...extra }) });
    }
  });

  await page.locator(".sidebar").getByRole("link", { name: "Views" }).click();
  await expect(page).toHaveURL(/\/acme\/views\/all$/);
  const rows = page.locator(".sheet-row:not(.sheet-head)");
  await expect(rows).toHaveCount(2);
  await expect(page.locator(".sheet-head")).toContainText("Project");
  await expect(rows.filter({ hasText: "Ship the beta" })).toContainText("Mobile");

  // Built-in views add their own condition.
  await page.locator(".view-tabs").getByRole("link", { name: "Assigned" }).click();
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText("Ship the beta");

  // Filters across projects, then saved as a view of its own.
  await page.locator(".view-tabs").getByRole("link", { name: "All work items" }).click();
  await openFilter(page, "Priority");
  await page.getByRole("option", { name: "Urgent" }).click();
  await page.keyboard.press("Escape");
  await expect(rows).toHaveCount(1);
  await page.getByRole("button", { name: "Add view" }).first().click();
  await page.getByRole("dialog").getByLabel("View name").fill("On fire");
  await page.getByRole("dialog").getByRole("button", { name: "Create view" }).click();
  await expect(page.locator(".view-tabs .active")).toHaveText("On fire");
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText("Fix the footer");

  // The title opens the item in the peek, with its own project's properties.
  await rows.getByRole("button", { name: "Fix the footer" }).click();
  await expect(page.getByRole("dialog", { name: "Properties" }).getByLabel("Title")).toHaveValue("Fix the footer");
  await page.getByRole("dialog", { name: "Properties" }).getByRole("button", { name: "Close" }).click();

  await page.getByRole("button", { name: "View actions" }).click();
  await page.getByRole("menuitem", { name: "Delete" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await expect(page).toHaveURL(/\/views\/all$/);
  await expect(page.locator(".view-tabs")).not.toContainText("On fire");
});

test("a member's profile: summary numbers, their work and their activity", async ({ page, hub }) => {
  await login(page, hub);
  const me = await page.evaluate(async () => (await (await fetch("/api/v1/me")).json()).id as string);
  await page.goto(`${hub.url}/acme/settings/members`);
  await page.locator(".member-link", { hasText: "Ana" }).click();
  await expect(page).toHaveURL(new RegExp(`/acme/profile/${me}$`));
  const overview = page.locator(".profile-tiles").first();
  await expect(overview.locator(".profile-tile-link", { hasText: "Created" })).toContainText("2");
  await expect(overview.locator(".profile-tile-link", { hasText: "Assigned" })).toContainText("1");
  await expect(page.locator(".profile-side")).toContainText("Owner");
  await page.locator(".profile-tabs").getByRole("link", { name: "Assigned" }).click();
  await expect(page.locator(".profile-main .row-link")).toHaveCount(1);
  await expect(page.locator(".profile-main .row-link")).toContainText("Ship the beta");
  await page.locator(".profile-tabs").getByRole("link", { name: "Activity" }).click();
  await expect(page.locator(".profile-activity li").first()).toContainText("Ana");
  await expect(page.getByRole("link", { name: "Download today's activity" })).toHaveAttribute("href", /format=csv/);
});
