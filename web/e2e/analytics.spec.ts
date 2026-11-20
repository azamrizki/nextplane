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
    const call = async (url: string, body: object, method = "POST") => (await fetch(url, { method, headers: h, body: JSON.stringify(body) })).json();
    const p = await call("/api/v1/workspaces/acme/projects", { name: "Website" });
    await call(`/api/v1/workspaces/acme/projects/${p.id}`, { estimate_type: "points" }, "PATCH");
    const states = await (await fetch(`/api/v1/workspaces/acme/projects/${p.id}/states`)).json();
    const done = states.find((s: { group: string }) => s.group === "completed").id;
    const started = states.find((s: { group: string }) => s.group === "started").id;
    const me = await (await fetch("/api/v1/me")).json();
    const bug = await call(`/api/v1/workspaces/acme/projects/${p.id}/labels`, { name: "Bug", color: "#e5484d" });
    const items: object[] = [
      { title: "Checkout fails", priority: "urgent", label_ids: [bug.id], assignee_ids: [me.id], estimate: "5" },
      { title: "Slow search", priority: "high", state_id: started, assignee_ids: [me.id], estimate: "3" },
      { title: "Typo on pricing", priority: "low", state_id: done, label_ids: [bug.id], estimate: "1" },
      { title: "New onboarding", priority: "medium", estimate: "8" },
      { title: "Old invoice bug", priority: "high", due_date: "2020-01-01", label_ids: [bug.id] },
    ];
    for (const i of items) await call(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, i);
    return p.id as string;
  });
  projectUrl = `${hub.url}/acme/projects/${pid}`;
});

test("overview: tiles, trend, breakdowns and their tables", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(hub.url + "/acme");
  await page.getByRole("link", { name: "Analytics" }).click();
  const tile = (label: string) => page.locator(".stat-tile", { hasText: label }).locator(".stat-value");
  await expect(tile("Open work items")).toHaveText("4");
  await expect(tile("Completed in the last 30 days")).toHaveText("1");
  await expect(tile("Overdue")).toHaveText("1");
  await expect(tile("Open points")).toHaveText("16");
  await expect(page.locator(".chart-legend").first()).toContainText("Created");
  const byPrio = page.locator(".chart-card", { hasText: "By priority" });
  await expect(byPrio.locator(".bar-row")).toHaveCount(4);
  await byPrio.locator(".bar-row", { hasText: "High" }).hover();
  await expect(byPrio.locator(".chart-tip")).toContainText("2 · 40%");
  if (shots) await page.screenshot({ path: `${shots}/analytics-light.png`, fullPage: true });
  // The same numbers as a table.
  await byPrio.getByRole("button", { name: "Table" }).click();
  await expect(byPrio.getByRole("row", { name: /High/ })).toContainText("2");
  // Trend: keyboard focus shows this week's numbers.
  const trend = page.locator(".chart-card", { hasText: "Created and completed per week" });
  await trend.locator("svg").focus();
  await expect(trend.locator(".chart-tip")).toContainText("5");
});

test("custom: points by label, segmented by priority, in dark mode", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/analytics");
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  await page.getByRole("tab", { name: "Custom" }).click();
  await page.getByLabel("Group by").selectOption("label");
  await page.getByLabel("Measure").selectOption("points");
  const card = page.locator(".chart-card");
  await expect(card.locator(".chart-title")).toHaveText("Estimate points by label, segmented by priority");
  await expect(card.locator(".chart-legend li")).toHaveCount(4);
  await expect(card.locator(".bar-row", { hasText: "Bug" }).locator(".bar-value")).toHaveText("6");
  await expect(card.locator(".bar-row", { hasText: "No label" }).locator(".bar-value")).toHaveText("11");
  await card.locator(".bar-row", { hasText: "Bug" }).locator(".stack-seg").first().hover();
  await expect(card.locator(".chart-tip")).toContainText("Bug");
  if (shots) await page.screenshot({ path: `${shots}/analytics-dark.png`, fullPage: true });
  await card.getByRole("button", { name: "Table" }).click();
  await expect(card.getByRole("columnheader", { name: "Urgent" })).toBeVisible();
});
