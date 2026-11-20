import { ADMIN, expect, test } from "./fixtures";

test.describe.configure({ mode: "serial" });

test("setup wizard creates the admin and lands on home", async ({ page, hub }) => {
  await page.goto(hub.url + "/");
  await expect(page).toHaveURL(/\/setup$/);
  await expect(page.getByRole("heading", { name: "This setup link does not work" })).toBeVisible();

  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill(ADMIN.name);
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  if (process.env.NEXTPLANE_E2E_SHOTS) await page.screenshot({ path: `${process.env.NEXTPLANE_E2E_SHOTS}/setup.png` });
  await page.getByRole("button", { name: "Create admin and continue" }).click();

  await expect(page).toHaveURL(/\/create-workspace$/);
  await expect(page.getByRole("heading", { name: "You are not in any workspace yet" })).toBeVisible();
});

test("logout, wrong password, login", async ({ page, hub }) => {
  await page.goto(hub.url + "/login");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill("not it at all");
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page.getByRole("alert")).toHaveText("Wrong email or password.");
  if (process.env.NEXTPLANE_E2E_SHOTS) await page.screenshot({ path: `${process.env.NEXTPLANE_E2E_SHOTS}/login-error.png` });

  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(page).toHaveURL(/\/create-workspace$/);

  await page.goto(hub.url + "/profile");
  await page.getByLabel("Theme").selectOption("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByLabel("Theme").selectOption("light");

  // Log out through the API the menu uses; the menu itself is covered in workspace.spec.ts.
  await page.evaluate(() => fetch("/api/v1/auth/logout", { method: "POST" }));
  await page.goto(hub.url + "/");
  await expect(page).toHaveURL(/\/login$/);
});

test("the used setup link no longer works", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await expect(page.getByRole("heading", { name: "Setup is already complete." })).toBeVisible();
});
