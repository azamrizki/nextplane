import { readFileSync } from "node:fs";
import { ADMIN, expect, test, choose } from "./fixtures";

test("public boards take comments, reactions and votes; exports span projects", async ({ page, hub, browser }) => {
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
    await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Dark mode" }) });
    return p.id as string;
  });

  // Publish, then let visitors comment, react and vote.
  await page.goto(`${hub.url}/acme/projects/${pid}/settings/publish`);
  await page.getByRole("button", { name: "Publish" }).click();
  for (const name of ["Allow comments", "Allow reactions", "Allow votes"]) {
    await page.getByRole("switch", { name }).check();
    await expect(page.getByRole("switch", { name })).toBeChecked();
  }
  const link = await page.locator(".copy-row input").inputValue();

  // A visitor without an account reads, and is asked to sign in to take part.
  const visitor = await (await browser.newContext()).newPage();
  await visitor.goto(link);
  await visitor.getByText("Dark mode").click();
  await expect(visitor.getByRole("button", { name: "Sign in to comment, react or vote" })).toBeVisible();
  await visitor.context().close();

  // Signed in, the same board takes part.
  await page.goto(link);
  await page.getByText("Dark mode").click();
  const dlg = page.getByRole("dialog");
  await dlg.getByRole("button", { name: "Vote up" }).click();
  await expect(dlg.getByRole("button", { name: "Vote up" })).toHaveAttribute("aria-pressed", "true");
  await dlg.getByRole("button", { name: "React" }).first().click();
  await page.getByRole("menuitem", { name: "🎉" }).click();
  await expect(dlg.locator(".reaction.mine", { hasText: "🎉" })).toBeVisible();
  await dlg.getByLabel("Add a comment").fill("Please add this <soon>");
  await dlg.getByRole("button", { name: "Comment", exact: true }).click();
  await expect(dlg.locator(".board-comments .comment")).toContainText("Please add this <soon>");

  // The team sees the comment in the app, marked public.
  await page.goto(`${hub.url}/acme/projects/${pid}`);
  await page.locator(".list-row", { hasText: "Dark mode" }).click();
  const peek = page.getByRole("dialog", { name: "Properties" });
  await expect(peek.locator(".comment", { hasText: "Please add this" }).locator(".badge")).toHaveText("Public");

  // Exports: every project you work in, downloaded at once and kept in the history.
  await page.goto(`${hub.url}/acme/settings/exports`);
  await page.getByRole("button", { name: "Export" }).first().click();
  await choose(page.getByRole("dialog").getByLabel("Format"), "csv");
  const download = page.waitForEvent("download");
  await page.getByRole("dialog").getByRole("button", { name: "Export" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/^acme-work-items-\d{4}-\d{2}-\d{2}\.csv$/);
  const text = readFileSync(await file.path(), "utf8");
  expect(text).toContain("Website,WEB,WEB-1,Dark mode");
  await expect(page.locator(".exports-table tbody tr")).toHaveCount(1);
  await expect(page.locator(".exports-table")).toContainText("All your projects");
});
