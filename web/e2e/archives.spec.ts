import { ADMIN, expect, login, test, choose } from "./fixtures";

test.describe.configure({ mode: "serial" });
let projectUrl = "";

const day = (offset: number) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

test("only closed work items are archived; the archive lists and restores them", async ({ page, hub }) => {
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
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website", feature_cycles: true }) })).json();
    const base = `/api/v1/workspaces/acme/projects/${p.id}`;
    await fetch(base, { method: "PATCH", headers: h, body: JSON.stringify({ feature_cycles: true }) });
    const states = await (await fetch(`${base}/states`)).json();
    const done = states.find((s: { group: string }) => s.group === "completed").id;
    await fetch(`${base}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Ship login", state_id: done }) });
    await fetch(`${base}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Draft spec" }) });
    return p.id as string;
  });
  projectUrl = `${hub.url}/acme/projects/${pid}`;
  await page.goto(projectUrl);

  // An open item cannot be archived: the entry is there, disabled, and says why.
  const open = page.locator(".list-row", { hasText: "Draft spec" });
  await open.hover();
  await open.getByRole("button", { name: "Work item actions" }).click();
  await expect(page.getByRole("menuitem", { name: /Archive/ })).toBeDisabled();
  await expect(page.getByRole("menuitem", { name: /Archive/ })).toContainText("Only completed or cancelled");
  await page.keyboard.press("Escape");

  const done = page.locator(".list-row", { hasText: "Ship login" });
  await done.hover();
  await done.getByRole("button", { name: "Work item actions" }).click();
  await page.getByRole("menuitem", { name: /Archive/ }).click();
  await expect(page.getByRole("dialog")).toContainText("Archive WEB-1?");
  await page.getByRole("dialog").getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".toast")).toContainText("WEB-1 archived");
  await expect(page.locator(".list-row", { hasText: "Ship login" })).toHaveCount(0);
  await page.locator(".toast").getByRole("link", { name: "View archives" }).click();
  await expect(page).toHaveURL(/\/archives$/);
  await expect(page.locator(".archive-tabs")).toContainText("Cycles");
  await expect(page.locator(".count-chip")).toHaveText("1");

  // The archived item opens read-only, with the way back.
  const row = page.locator(".list-row", { hasText: "Ship login" });
  await row.click();
  const peek = page.getByRole("dialog", { name: "Properties" });
  await expect(peek.locator(".wi-archived")).toContainText("This work item is archived");
  await expect(peek.getByLabel("Title")).toHaveAttribute("readonly", "");

  // Restore from the row menu: back in the project's list.
  await peek.getByRole("button", { name: "Close" }).click();
  await row.hover();
  await row.getByRole("button", { name: "Work item actions" }).click();
  await page.getByRole("menuitem", { name: "Restore" }).click();
  await expect(page.locator(".toast").last()).toContainText("WEB-1 restored");
  await expect(page.getByText("No archived work items yet")).toBeVisible();
  await page.goto(projectUrl);
  await expect(page.locator(".list-row", { hasText: "Ship login" })).toBeVisible();
});

test("a finished cycle is archived from its page and restored from the archive", async ({ page, hub }) => {
  await login(page, hub);
  const id = await page.evaluate(
    async ({ url, d1, d2 }) => {
      const base = "/api/v1" + new URL(url).pathname.replace(/^\//, "/workspaces/");
      const r = await fetch(`${base}/cycles`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: "Sprint 1", start_date: d1, end_date: d2 }) });
      return (await r.json()).id as string;
    },
    { url: projectUrl, d1: day(-20), d2: day(-6) },
  );
  await page.goto(`${projectUrl}/cycles/${id}`);
  await page.locator(".cycle-title").getByRole("button", { name: "Archive" }).click();
  await expect(page).toHaveURL(/\/cycles$/);
  await expect(page.locator(".toast")).toContainText("Cycle archived");
  await page.goto(`${projectUrl}/archives/cycles`);
  await expect(page.locator(".cycle-row", { hasText: "Sprint 1" })).toBeVisible();
  await page.locator(".cycle-row", { hasText: "Sprint 1" }).getByRole("button", { name: "Restore" }).click();
  await expect(page.getByText("No archived cycles yet")).toBeVisible();
});

test("project admins set up automations in project settings", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(`${projectUrl}/settings/automations`);
  await expect(page.getByRole("heading", { level: 1, name: "Automations" })).toBeVisible();
  const archive = page.getByRole("switch", { name: "Auto-archive closed work items" });
  await archive.check();
  await expect(page.getByLabel("Months").first()).toHaveAttribute("data-value", "1");
  await choose(page.getByLabel("Months").first(), "custom");
  await page.getByRole("dialog").getByLabel("Months").fill("13");
  await expect(page.getByRole("dialog").getByRole("button", { name: "Save" })).toBeDisabled();
  await page.getByRole("dialog").getByLabel("Months").fill("4");
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await expect(page.getByLabel("Months").first()).toHaveAttribute("data-value", "4");
  const close = page.getByRole("switch", { name: "Auto-close work items" });
  await close.check();
  await expect(page.getByLabel("Close into")).toHaveAttribute("data-value", /.+/);
  await expect(page.getByLabel("Close into")).toHaveText("Cancelled");
  // The settings are saved on the project.
  const p = await page.evaluate(async (url) => (await fetch("/api/v1" + new URL(url).pathname.replace(/^\//, "/workspaces/"))).json(), projectUrl);
  expect([p.archive_in, p.close_in]).toEqual([4, 1]);
  await close.uncheck();
  await expect(page.getByLabel("Close into")).toHaveCount(0);
});
