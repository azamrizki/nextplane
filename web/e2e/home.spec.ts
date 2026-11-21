import { ADMIN, expect, test } from "./fixtures";

test("home widgets: quick links, recents, stickies, and managing them", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana Lima");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  await expect(page.locator(".home-hello h2")).toContainText("Ana Lima");
  await expect(page.getByRole("region", { name: "Get started" })).toContainText("Create a project");

  // Quick links.
  const links = page.getByRole("region", { name: "Quick links" });
  await links.getByRole("button", { name: "Add quick link" }).click();
  await page.getByRole("dialog").getByLabel("URL").fill("docs.example.com/handbook");
  await page.getByRole("dialog").getByLabel("Title (optional)").fill("Handbook");
  await page.getByRole("dialog").getByRole("button", { name: "Add quick link" }).click();
  const card = links.locator(".link-card", { hasText: "Handbook" });
  await expect(card).toContainText("docs.example.com");
  await expect(card.locator("a").first()).toHaveAttribute("href", "http://docs.example.com/handbook");

  // Recents follow what you open.
  const pid = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Fix the footer" }) });
    return p.id as string;
  });
  await page.goto(`${hub.url}/acme/projects/${pid}`);
  await page.locator(".list-row", { hasText: "Fix the footer" }).click();
  await expect(page.getByRole("dialog", { name: "Properties" })).toBeVisible();
  await page.goto(`${hub.url}/acme`);
  const recents = page.getByRole("region", { name: "Recents" });
  await expect(recents.locator(".recent-row").first()).toContainText("Fix the footer");
  await expect(recents.locator(".recent-row").nth(1)).toContainText("Website");
  await recents.getByRole("button", { name: "Show: All" }).click();
  await page.getByRole("menuitem", { name: "Projects" }).click();
  await expect(recents.locator(".recent-row")).toHaveCount(1);

  // Stickies: write, colour, and the empty-sticky rule.
  const stickies = page.getByRole("region", { name: "Your stickies" });
  await stickies.getByRole("button", { name: "Add sticky" }).click();
  const note = stickies.getByRole("textbox", { name: "Sticky note" }).first();
  await expect(note).toBeVisible();
  await stickies.getByRole("button", { name: "Add sticky" }).click();
  await expect(page.locator(".toast")).toContainText("Write something in your newest sticky first");
  await note.click();
  await page.keyboard.type("Call the bank");
  await stickies.getByRole("button", { name: "Colour" }).first().click();
  await page.getByRole("menuitemradio", { name: "Green" }).click();
  await expect(stickies.locator(".sticky").first()).toHaveClass(/sticky-green/);
  await expect.poll(async () => page.evaluate(async () => (await (await fetch("/api/v1/workspaces/acme/stickies?q=bank")).json()).length)).toBe(1);
  await page.locator(".sidebar").getByRole("link", { name: "Stickies" }).click();
  await expect(page.getByRole("textbox", { name: "Sticky note" })).toHaveText("Call the bank");

  // Manage widgets: hide one, move another to the top.
  await page.goto(`${hub.url}/acme`);
  await page.getByRole("button", { name: "Manage widgets" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByRole("switch", { name: "Recents" }).uncheck();
  await dlg.getByRole("button", { name: "Move Your stickies up" }).click();
  await dlg.getByRole("button", { name: "Move Your stickies up" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("region", { name: "Recents" })).toHaveCount(0);
  await expect(page.locator(".home-widgets > section").first()).toHaveAttribute("aria-label", "Your stickies");
  await page.reload();
  await expect(page.locator(".home-widgets > section").first()).toHaveAttribute("aria-label", "Your stickies");
});
