import { ADMIN, expect, login, test } from "./fixtures";

test.describe.configure({ mode: "serial" });
const shots = process.env.NEXTPLANE_E2E_SHOTS;

test("create a project and configure states, labels, estimates and features", async ({ page, hub }) => {
  // Setup + workspace through the API to keep this test about projects.
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);

  await page.getByRole("link", { name: "Projects", exact: true }).click();
  await page.getByRole("button", { name: "New project" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Project name").fill("Mobile App");
  await expect(dlg.getByLabel("Identifier")).toHaveValue("MA");
  await dlg.getByLabel("Description (optional)").fill("The Android and iOS apps.");
  await dlg.getByRole("button", { name: "Create project" }).click();
  await expect(page.locator(".crumbs").getByRole("link", { name: "Mobile App" })).toBeVisible();
  await expect(page.locator(".sidebar .side-project-toggle", { hasText: "Mobile App" })).toBeVisible();

  await page.locator(".sidebar").getByRole("button", { name: "Mobile App actions" }).click();
  await page.getByRole("menuitem", { name: "Settings" }).click();
  await page.getByRole("link", { name: "States" }).click();
  const started = page.locator(".group-head").filter({ has: page.getByText("Started", { exact: true }) });
  await started.getByRole("button", { name: "+ Add state" }).click();
  await page.getByRole("dialog").getByLabel("State name").fill("In Review");
  await page.getByRole("dialog").getByRole("button", { name: "Add" }).click();
  await expect(page.locator(".inline-input[value='In Review']")).toHaveCount(1);
  if (shots) await page.screenshot({ path: `${shots}/project-states.png` });

  await page.getByRole("link", { name: "Labels" }).click();
  await page.getByRole("button", { name: "Add label" }).click();
  await page.getByRole("dialog").getByLabel("Label name").fill("Bug");
  await page.getByRole("dialog").getByRole("button", { name: "Add" }).click();
  await expect(page.locator(".inline-input[value='Bug']")).toHaveCount(1);

  await page.getByRole("link", { name: "Estimates" }).click();
  await page.getByLabel("Points").check();
  await expect(page.getByLabel("Values")).toHaveValue("1, 2, 3, 5, 8, 13");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved")).toBeVisible();

  await page.getByRole("link", { name: "Features" }).click();
  const intake = page.locator("label.row", { hasText: "Intake" }).locator("input");
  await expect(intake).not.toBeChecked();
  await intake.check();
  await expect(intake).toBeChecked();
});

test("a member sees public projects and joins; private stays hidden", async ({ page, browser, hub }) => {
  await login(page, hub);
  // Private project + an invite for Bob, via the API with the admin's session.
  const res = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Secret", is_private: true }) });
    const r = await fetch("/api/v1/workspaces/acme/invites", { method: "POST", headers: h, body: JSON.stringify({ invites: [{ email: "bob@example.com", role: "member" }] }) });
    return (await r.json())[0].link as string;
  });
  const bobCtx = await browser.newContext();
  const bob = await bobCtx.newPage();
  await bob.goto(res.replace(/^https?:\/\/[^/]+/, hub.url));
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByLabel("Password").fill("password123");
  await bob.getByRole("button", { name: "Create your account" }).click();
  await expect(bob).toHaveURL(/\/acme$/);

  await bob.getByRole("link", { name: "Projects", exact: true }).click();
  await expect(bob.getByText("Mobile App")).toBeVisible();
  await expect(bob.getByText("Secret")).toHaveCount(0);
  if (shots) await bob.screenshot({ path: `${shots}/projects-list.png` });
  await bob.getByRole("button", { name: "Join" }).click();
  await expect(bob.locator(".crumbs").getByRole("link", { name: "Mobile App" })).toBeVisible();
  await bob.locator(".sidebar").getByRole("button", { name: "Mobile App actions" }).click();
  await bob.getByRole("menuitem", { name: "Settings" }).click();
  await expect(bob.getByLabel("Project name")).toBeDisabled();
  await bobCtx.close();
});
