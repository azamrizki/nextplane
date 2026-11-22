import { ADMIN, expect, test, choose } from "./fixtures";

const shots = process.env.NEXTPLANE_E2E_SHOTS;

test("create a module, add items from inside and from the item, see progress", async ({ page, hub }) => {
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
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Search box" }) });
    return p.id as string;
  });
  const projectUrl = `${hub.url}/acme/projects/${pid}`;
  await page.goto(projectUrl);
  await page.getByRole("link", { name: "Modules" }).click();
  await page.getByRole("button", { name: "New module" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Module name").fill("Checkout");
  await choose(dlg.getByLabel("Status"), "in_progress");
  await dlg.getByRole("button", { name: "New module" }).click();
  await expect(page.locator(".cycle-title")).toContainText("Checkout");
  await expect(page.locator(".cycle-title")).toContainText("In progress");

  await page.locator('.list-add[data-group="Todo"]').click();
  await page.locator(".list-row-form").getByLabel("Title").fill("Card payments");
  await page.locator(".list-row-form").getByLabel("Title").press("Enter");
  await expect(page.locator(".list-row", { hasText: "Card payments" })).toBeVisible();
  await expect(page.locator(".cycle-progress")).toContainText("0 of 1 done");

  // Add an existing item from its own page.
  await page.goto(projectUrl);
  await page.locator(".list-row", { hasText: "Search box" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Modules" }).click();
  await page.getByRole("option", { name: /Checkout/ }).click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.getByRole("link", { name: "Modules" }).click();
  await expect(page.locator(".mod-card")).toContainText("0 of 2 done");
  if (shots) await page.screenshot({ path: `${shots}/modules.png` });
  await page.locator(".mod-card").click();
  await expect(page.locator(".list-row")).toHaveCount(2);
});
