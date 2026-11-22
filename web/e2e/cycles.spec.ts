import { ADMIN, expect, login, test, openFilter, pickRange } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";

const day = (offset: number) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

test("create a cycle, add items, see progress and burn-down", async ({ page, hub }) => {
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
    const r = await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: "Website" }) });
    return (await r.json()).id as string;
  });
  projectUrl = `${hub.url}/acme/projects/${pid}`;
  await page.goto(projectUrl);
  await page.getByRole("link", { name: "Cycles" }).click();
  await expect(page.getByText("No cycles yet")).toBeVisible();
  await page.getByRole("button", { name: "New cycle" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Cycle name").fill("Sprint 12");
  await pickRange(dlg.getByLabel("Start date"), day(-4), day(9));
  await dlg.getByRole("button", { name: "New cycle" }).click();
  await expect(page.locator(".cycle-title")).toContainText("Sprint 12");
  await expect(page.locator(".cycle-title .badge")).toHaveText("Current");

  // Quick add inside the cycle puts items in it.
  await page.locator('.list-add[data-group="Todo"]').click();
  const input = page.locator(".list-row-form").getByLabel("Title");
  for (const title of ["Checkout redesign", "Fix coupon rounding", "Update copy"]) {
    await input.fill(title);
    await input.press("Enter");
    await expect(page.locator(".list-row", { hasText: title })).toBeVisible();
  }
  await page.keyboard.press("Escape");
  await page.locator(".list-row", { hasText: "Update copy" }).getByRole("button", { name: "State" }).click();
  await page.getByRole("option", { name: "Done" }).click();
  await expect(page.locator(".cycle-progress")).toContainText("1 of 3 done");
  await expect(page.locator(".cycle-progress")).toContainText("33% complete");
  await expect(page.getByRole("img", { name: /Burn-down/ })).toBeVisible();
  await page.getByRole("img", { name: /Burn-down/ }).hover();
  await expect(page.locator(".chart-tip")).toContainText("Remaining");
  if (shots) await page.screenshot({ path: `${shots}/cycle.png` });

  // The work items page shows the cycle chip and filter.
  await page.goto(projectUrl);
  await expect(page.locator(".list-row")).toHaveCount(3);
  await openFilter(page, "Cycle");
  await page.getByRole("option", { name: /No cycle/ }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator(".list-row")).toHaveCount(0);
  await page.getByRole("button", { name: "Clear filters" }).click();
});

test("move unfinished items to the next cycle", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(projectUrl + "/cycles");
  await page.getByRole("button", { name: "New cycle" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Cycle name").fill("Sprint 13");
  await pickRange(dlg.getByLabel("Start date"), day(10), day(23));
  await dlg.getByRole("button", { name: "New cycle" }).click();
  await expect(page.locator(".cycle-title .badge")).toHaveText("Upcoming");
  await page.getByRole("link", { name: "Cycles", exact: true }).first().click();
  await expect(page.getByRole("region", { name: "Current" })).toContainText("Sprint 12");
  await expect(page.getByRole("region", { name: "Upcoming" })).toContainText("Sprint 13");
  if (shots) await page.screenshot({ path: `${shots}/cycles-list.png` });
  await page.getByRole("link", { name: /Sprint 12/ }).click();
  await page.getByRole("button", { name: "Move unfinished items" }).click();
  await page.getByRole("dialog").getByRole("button", { name: /Sprint 13/ }).click();
  await expect(page.getByText("2 items moved.")).toBeVisible();
  await expect(page.locator(".cycle-progress")).toContainText("1 of 1 done");
});
