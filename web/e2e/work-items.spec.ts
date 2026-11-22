import { ADMIN, expect, test, openFilter, pickDate } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;
let projectUrl = "";

test("quick add, peek, edit properties and description", async ({ page, hub }) => {
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
    const p = await r.json();
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/labels`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: "Bug", color: "#e5484d" }) });
    return p.id as string;
  });
  projectUrl = `${hub.url}/acme/projects/${pid}`;
  await page.goto(projectUrl);

  // Quick add in "Todo".
  await page.locator('.list-add[data-group="Todo"]').click();
  const input = page.locator(".list-row-form").getByLabel("Title");
  await input.fill("Fix login redirect");
  await input.press("Enter");
  await input.fill("Write release notes");
  await input.press("Enter");
  const row = page.locator('.list-row[data-group="Todo"]', { hasText: "Fix login redirect" });
  await expect(row).toBeVisible();
  await expect(row.locator(".list-key")).toHaveText(/WEB-\d/);

  // Open the peek panel and edit.
  await row.click();
  const peek = page.getByRole("dialog");
  await expect(peek.getByLabel("Title")).toHaveValue("Fix login redirect");
  await peek.getByRole("button", { name: "Priority" }).click();
  await page.getByRole("option", { name: "Urgent" }).click();
  await peek.getByRole("button", { name: "State" }).click();
  await page.getByRole("option", { name: "In Progress" }).click();
  await peek.getByRole("button", { name: "Labels" }).click();
  await page.getByRole("option", { name: "Bug" }).click();
  await page.keyboard.press("Escape");
  await peek.getByRole("button", { name: "Assignees" }).click();
  await page.getByRole("option", { name: "Ana" }).click();
  await page.keyboard.press("Escape");
  await pickDate(peek.getByLabel("Due date"), "2026-12-24");

  const editor = peek.getByRole("textbox", { name: "Description" });
  await editor.click();
  await editor.pressSequentially("Users land on a blank page after login.");
  await peek.getByLabel("Title").click(); // blur saves the description
  await expect(peek.locator(".activity")).toContainText("changed the description");
  await expect(peek.locator(".activity")).toContainText("changed the priority to Urgent");
  if (shots) await page.screenshot({ path: `${shots}/work-item-peek.png` });

  // The list shows the new state.
  await page.keyboard.press("Escape");
  await expect(page.locator('.list-row[data-group="In Progress"]', { hasText: "Fix login redirect" })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/work-items-list.png` });
});

test("full page: sub-item, relation, reload keeps the description", async ({ page, hub }) => {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).not.toHaveURL(/login/);
  await page.goto(projectUrl);
  await page.locator(".list-row", { hasText: "Fix login redirect" }).first().click();
  await page.getByRole("link", { name: "Open full page" }).click();
  await expect(page).toHaveURL(/\/work-items\//);
  await expect(page.getByRole("textbox", { name: "Description" })).toContainText("blank page after login");

  await page.getByRole("button", { name: "+ Add sub-item" }).click();
  await page.locator(".wi-section").getByLabel("Title").fill("Add a regression test");
  await page.locator(".wi-section").getByLabel("Title").press("Enter");
  await expect(page.locator(".wi-section", { hasText: "Sub-items" }).getByText("Add a regression test")).toBeVisible();

  await page.getByRole("button", { name: "+ Add relation" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByRole("button", { name: "Blocked by" }).click();
  await dlg.getByLabel("Search work items").fill("release");
  await dlg.getByRole("button", { name: /Write release notes/ }).click();
  await expect(page.locator(".wi-section", { hasText: "Relations" })).toContainText("Blocked by");
  if (shots) await page.screenshot({ path: `${shots}/work-item-full.png` });

  await page.reload();
  await expect(page.locator(".wi-section", { hasText: "Relations" })).toContainText("Write release notes");
});

test("create dialog with the C shortcut", async ({ page, hub }) => {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).not.toHaveURL(/login/);
  await page.goto(projectUrl);
  await expect(page.locator(".list-row").first()).toBeVisible();
  await page.keyboard.press("c");
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Title").fill("Dark mode for settings");
  await dlg.getByRole("button", { name: "Priority" }).click();
  await page.getByRole("option", { name: "Low" }).click();
  await dlg.getByRole("button", { name: "Create work item" }).click();
  await expect(page.getByRole("dialog").getByLabel("Title")).toHaveValue("Dark mode for settings");
});

test("kanban drag, filters and bulk edit", async ({ page, hub }) => {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).not.toHaveURL(/login/);
  await page.goto(projectUrl);

  // Filters: priority Urgent leaves one item.
  await openFilter(page, "Priority");
  await page.getByRole("option", { name: "Urgent" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator(".list-row")).toHaveCount(1);
  await page.getByRole("button", { name: "Clear filters" }).click();
  await expect(page.locator(".list-row")).toHaveCount(4);

  // Bulk: select two rows, set priority High.
  await page.locator(".list-row", { hasText: "Write release notes" }).locator(".row-check").check();
  await page.locator(".list-row", { hasText: "Dark mode for settings" }).locator(".row-check").check();
  const bar = page.getByRole("toolbar", { name: "2 selected" });
  await bar.getByRole("button", { name: "Priority" }).click();
  await page.getByRole("option", { name: "High" }).click();
  await bar.getByRole("button", { name: "Clear selection" }).click();
  await openFilter(page, "Priority");
  await page.getByRole("option", { name: "High" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator(".list-row")).toHaveCount(2);
  await page.getByRole("button", { name: "Clear filters" }).click();

  // Kanban: drag "Write release notes" from Todo to Done. A wide window keeps all five
  // columns on screen, so the board does not auto-scroll under the pointer.
  await page.setViewportSize({ width: 1900, height: 900 });
  await page.getByRole("tab", { name: "Board" }).click();
  const card = page.locator(".board-card", { hasText: "Write release notes" });
  const done = page.getByRole("region", { name: "Done" }).locator(".board-col-body");
  await card.hover();
  await page.mouse.down();
  const box = (await done.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + 20, { steps: 12 });
  await page.mouse.move(box.x + box.width / 2, box.y + 30, { steps: 4 });
  await page.mouse.up();
  await expect(page.getByRole("region", { name: "Done" }).locator(".board-card", { hasText: "Write release notes" })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/kanban.png` });
  await page.reload();
  await expect(page.getByRole("region", { name: "Done" }).locator(".board-card", { hasText: "Write release notes" })).toBeVisible();
  await page.getByRole("tab", { name: "List" }).click();
  await page.setViewportSize({ width: 1280, height: 800 });
});

test("comments with a mention, reactions, attachment upload, link", async ({ page, hub }) => {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).not.toHaveURL(/login/);
  await page.goto(projectUrl);
  await page.locator(".list-row", { hasText: "Fix login redirect" }).first().click();
  await page.getByRole("link", { name: "Open full page" }).click();

  const box = page.locator(".comment-box").getByRole("textbox");
  await box.click();
  await box.pressSequentially("Can you check this @An");
  await expect(page.locator(".mention-menu")).toBeVisible();
  await page.keyboard.press("Enter");
  await box.pressSequentially(" thanks");
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  const comment = page.locator(".comment").first();
  await expect(comment.locator(".mention")).toHaveText("@Ana");
  await comment.getByRole("button", { name: "Add reaction" }).click();
  await page.getByRole("menuitem", { name: "👍" }).click();
  await expect(comment.locator(".reaction.mine")).toContainText("1");

  await page.locator('input[type="file"]').setInputFiles({ name: "log.txt", mimeType: "text/plain", buffer: Buffer.from("error at line 3") });
  await expect(page.locator(".file", { hasText: "log.txt" })).toBeVisible();

  await page.getByRole("button", { name: "+ Add link" }).click();
  await page.getByLabel("URL").fill("https://github.com/acme/web/pull/7");
  await page.getByLabel("Title", { exact: true }).last().fill("Fix PR");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByRole("link", { name: "Fix PR" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Unsubscribe/ })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/comments.png`, fullPage: true });
});
