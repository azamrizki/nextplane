import { ADMIN, expect, login, test, openFilter } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";

const day = (offset: number) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

test("setup with dated items", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const pid = await page.evaluate(async ([d1, d2, d3]) => {
    const h = { "Content-Type": "application/json" };
    const me = await (await fetch("/api/v1/me")).json();
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    const w = `/api/v1/workspaces/acme/projects/${p.id}/work-items`;
    await fetch(w, { method: "POST", headers: h, body: JSON.stringify({ title: "Design review", start_date: d1, due_date: d2, assignee_ids: [me.id] }) });
    await fetch(w, { method: "POST", headers: h, body: JSON.stringify({ title: "Launch email", due_date: d3, priority: "high" }) });
    await fetch(w, { method: "POST", headers: h, body: JSON.stringify({ title: "Someday idea" }) });
    return p.id as string;
  }, [day(0), day(3), day(5)]);
  projectUrl = `${hub.url}/acme/projects/${pid}`;
});

test("spreadsheet edits a cell", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await page.getByRole("tab", { name: "Spreadsheet" }).click();
  const row = page.getByRole("row", { name: /Launch email/ });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Priority" }).click();
  await page.getByRole("option", { name: "Urgent" }).click();
  await expect(row.getByRole("button", { name: "Priority" })).toContainText("Urgent");
  if (shots) await page.screenshot({ path: `${shots}/spreadsheet.png` });
});

test("calendar moves a due date by dragging", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await page.getByRole("tab", { name: "Calendar" }).click();
  const item = page.locator(".cal-item", { hasText: "Launch email" });
  await expect(item).toBeVisible();
  const target = page.getByRole("gridcell", { name: day(6) });
  if (!(await target.isVisible())) test.skip(true, "day 6 is in the next month view");
  await item.hover();
  await page.mouse.down();
  const box = (await target.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 10 });
  await page.mouse.up();
  await expect(target.locator(".cal-item", { hasText: "Launch email" })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/calendar.png` });
});

test("timeline resizes a bar", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await page.getByRole("tab", { name: "Timeline" }).click();
  const bar = page.getByRole("button", { name: new RegExp(`Design review: ${day(0)} – ${day(3)}`) });
  await expect(bar).toBeVisible();
  const b = (await bar.boundingBox())!;
  await page.mouse.move(b.x + b.width - 3, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width - 3 + 28 * 2, b.y + b.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect(page.getByRole("button", { name: new RegExp(`Design review: ${day(0)} – ${day(5)}`) })).toBeVisible();
  await expect(page.getByText("Without dates (1)")).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/timeline.png` });
});

test("save a view and apply it; my work", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl);
  await page.getByRole("tab", { name: "List" }).click();
  await openFilter(page, "Priority");
  await page.getByRole("option", { name: "Urgent" }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("tab", { name: "Board" }).click();
  await page.getByRole("button", { name: /Views/ }).click();
  await page.getByRole("menuitem", { name: "Save as new view" }).click();
  await page.getByRole("dialog").getByLabel("View name").fill("Urgent board");
  await page.getByRole("dialog").getByRole("button", { name: "Save view" }).click();
  // Reset, then apply the saved view.
  await page.getByRole("button", { name: "Clear filters" }).click();
  await page.getByRole("tab", { name: "List" }).click();
  await page.getByRole("button", { name: /Urgent board/ }).click();
  await page.getByRole("menuitem", { name: /^Urgent board/ }).click();
  await expect(page.getByRole("tab", { name: "Board" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".board-card")).toHaveCount(1);

  await page.getByRole("link", { name: "Your work" }).click();
  await expect(page.locator(".row", { hasText: "Design review" })).toBeVisible();
  await page.getByRole("tab", { name: "Created" }).click();
  await expect(page.locator(".row-link")).toHaveCount(3);
});
