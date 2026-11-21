import { ADMIN, expect, login, test } from "./fixtures";

// Design review screenshots of the main screens in light and dark. Runs only when
// NEXTPLANE_SHOTS names an output folder: NEXTPLANE_SHOTS=/tmp/shots npx playwright test -c e2e/playwright.config.ts shots
const out = process.env.NEXTPLANE_SHOTS;
test.skip(!out, "set NEXTPLANE_SHOTS to an output folder to take screenshots");
test.describe.configure({ mode: "serial" });

let base = "";
let projectUrl = "";
let itemId = "";

const day = (offset: number) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

test("seed", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana Putri");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const r = await page.evaluate(async (d) => {
    const h = { "Content-Type": "application/json" };
    const j = async (url: string, body: unknown) => (await fetch(url, { method: "POST", headers: h, body: JSON.stringify(body) })).json();
    const me = await (await fetch("/api/v1/me")).json();
    const p = await j("/api/v1/workspaces/acme/projects", { name: "Website", icon: "🚀" });
    await j("/api/v1/workspaces/acme/projects", { name: "Mobile App", icon: "📱" });
    const pp = `/api/v1/workspaces/acme/projects/${p.id}`;
    await fetch(pp, { method: "PATCH", headers: h, body: JSON.stringify({ feature_cycles: true, feature_modules: true, feature_pages: true, feature_intake: true, estimate_type: "points", estimate_values: ["1", "2", "3", "5", "8"] }) });
    const states = await (await fetch(`${pp}/states`)).json();
    const st = (g: string) => states.find((s: { group: string }) => s.group === g).id;
    const bug = await j(`${pp}/labels`, { name: "Bug", color: "#e5484d" });
    const feat = await j(`${pp}/labels`, { name: "Feature", color: "#3e63dd" });
    const ux = await j(`${pp}/labels`, { name: "UX", color: "#8e4ec6" });
    const cyc = await j(`${pp}/cycles`, { name: "Sprint 12", start_date: d[0], end_date: d[3] });
    await j(`${pp}/modules`, { name: "Onboarding" });
    const items = [
      { title: "Fix login redirect after password reset", priority: "urgent", state_id: st("started"), label_ids: [bug.id], due_date: d[1], assignee_ids: [me.id], cycle_id: cyc.id, estimate: "3" },
      { title: "Write release notes for 0.2", priority: "medium", state_id: st("unstarted"), due_date: d[2], assignee_ids: [me.id] },
      { title: "Dark mode for the settings pages", priority: "low", state_id: st("backlog"), label_ids: [feat.id, ux.id] },
      { title: "Onboarding checklist for new members", priority: "high", state_id: st("started"), label_ids: [feat.id], start_date: d[0], due_date: d[3], cycle_id: cyc.id, estimate: "5" },
      { title: "Remove the old export endpoint", priority: "none", state_id: st("completed") },
      { title: "Investigate slow board on 10k items", priority: "high", state_id: st("unstarted"), label_ids: [bug.id], due_date: d[4] },
      { title: "Drop the legacy theme switch", priority: "low", state_id: st("cancelled") },
    ];
    let first = "";
    for (const it of items) {
      const w = await j(`${pp}/work-items`, it);
      first ||= w.id;
    }
    await j(`${pp}/pages`, { title: "Release plan" });
    return { pid: p.id as string, first };
  }, [day(-3), day(-1), day(2), day(9), day(14)]);
  base = `${hub.url}/acme`;
  projectUrl = `${base}/projects/${r.pid}`;
  itemId = r.first;
});

for (const theme of ["light", "dark"] as const) {
  test(`screens (${theme})`, async ({ page, hub }) => {
    await page.addInitScript((th) => localStorage.setItem("np.theme", th), theme);
    await login(page, hub);
    const shot = (name: string) => page.screenshot({ path: `${out}/${theme}-${name}.png` });
    await page.goto(base);
    await page.waitForTimeout(400);
    await shot("home");
    await page.goto(`${base}/projects`);
    await shot("projects");
    await page.goto(projectUrl);
    await page.getByRole("tab", { name: "List" }).click();
    await expect(page.locator(".list-row").first()).toBeVisible();
    await shot("list");
    await page.getByRole("tab", { name: "Board" }).click();
    await page.waitForTimeout(200);
    await shot("board");
    await page.getByRole("tab", { name: "Spreadsheet" }).click();
    await page.waitForTimeout(200);
    await shot("spreadsheet");
    await page.getByRole("tab", { name: "Calendar" }).click();
    await page.waitForTimeout(200);
    await shot("calendar");
    await page.getByRole("tab", { name: "Timeline" }).click();
    await page.waitForTimeout(200);
    await shot("timeline");
    await page.getByRole("tab", { name: "List" }).click();
    await page.getByRole("button", { name: "Display" }).click();
    await shot("display");
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: /^Filters/ }).click();
    await page.getByRole("button", { name: "Add filter" }).click();
    await shot("filter-menu");
    await page.getByRole("menuitem", { name: "Priority" }).click();
    await page.getByRole("option", { name: "Urgent" }).click();
    await page.getByRole("option", { name: "High" }).click();
    await page.keyboard.press("Escape");
    await shot("filtered");
    await page.getByRole("button", { name: "Clear filters" }).click();
    await page.goto(`${projectUrl}?peek=${itemId}`);
    await page.waitForTimeout(500);
    await shot("peek");
    await page.goto(`${projectUrl}/work-items/${itemId}`);
    await page.waitForTimeout(500);
    await shot("detail");
    await page.goto(projectUrl);
    await page.getByRole("button", { name: "Add work item", exact: true }).click();
    await page.waitForTimeout(300);
    await shot("create");
    await page.keyboard.press("Escape");
    await page.goto(`${projectUrl}/cycles`);
    await page.waitForTimeout(300);
    await shot("cycles");
    await page.locator(".cycle-title, .row-link, a", { hasText: "Sprint 12" }).first().click();
    await page.waitForTimeout(500);
    await shot("cycle");
    await page.goto(`${projectUrl}/modules`);
    await page.waitForTimeout(500);
    await shot("modules");
    await page.goto(`${projectUrl}/pages`);
    await page.waitForTimeout(800);
    await shot("pages");
    await page.goto(`${projectUrl}/intake`);
    await page.waitForTimeout(500);
    await shot("intake");
    await page.goto(`${projectUrl}/settings`);
    await shot("project-settings");
    await page.goto(`${base}/inbox`);
    await shot("inbox");
    await page.goto(`${base}/analytics`);
    await page.waitForTimeout(500);
    await shot("analytics");
    await page.goto(projectUrl);
    await page.getByRole("button", { name: "Switch workspace" }).click();
    await shot("ws-menu");
  });
}
