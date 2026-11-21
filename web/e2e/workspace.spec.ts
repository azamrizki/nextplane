import type { Browser, Page } from "@playwright/test";
import { ADMIN, expect, login, test, type Hub } from "./fixtures";

test.describe.configure({ mode: "serial" });

const shots = process.env.NEXTPLANE_E2E_SHOTS;

async function setupAndLogin(page: Page, hub: Hub) {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill(ADMIN.name);
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await expect(page).toHaveURL(/\/create-workspace$/);
}

async function newPage(browser: Browser, hub: Hub) {
  const ctx = await browser.newContext();
  const p = await ctx.newPage();
  await p.goto(hub.url + "/login");
  return p;
}

let inviteLink = "";

test("create a workspace, rename it, invite a member", async ({ page, hub }) => {
  await setupAndLogin(page, hub);
  await page.getByLabel("Workspace name").fill("Kopi Kenangan");
  await expect(page.getByLabel("Workspace URL")).toHaveValue("kopi-kenangan");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/kopi-kenangan$/);
  await expect(page.locator(".ws-switch-name")).toHaveText("Kopi Kenangan");

  await page.getByRole("button", { name: "Switch workspace" }).click();
  await page.getByRole("menuitem", { name: "Settings" }).click();
  await page.getByLabel("Workspace name").fill("Kopi");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved")).toBeVisible();
  await expect(page.locator(".ws-switch-name")).toHaveText("Kopi");

  await page.getByRole("link", { name: "Members" }).click();
  await page.getByRole("button", { name: "Invite people" }).click();
  await page.getByRole("dialog").getByLabel("Emails").fill("bob@example.com");
  await page.getByRole("dialog").getByLabel("Role").selectOption("member");
  await page.getByRole("button", { name: "Create invites" }).click();
  const code = page.locator(".link-box code");
  await expect(code).toContainText("/invite/");
  inviteLink = (await code.textContent())!;
  if (shots) await page.screenshot({ path: `${shots}/invite-links.png` });
  await page.getByRole("button", { name: "Close" }).first().click();
  await expect(page.getByText("Pending invites")).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/members.png` });
});

test("the invited person creates an account and lands in the workspace", async ({ browser, hub }) => {
  const bob = await newPage(browser, hub);
  await bob.goto(inviteLink.replace(/^https?:\/\/[^/]+/, hub.url));
  await expect(bob.getByRole("heading", { name: "Join Kopi" })).toBeVisible();
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByLabel("Password").fill("password123");
  if (shots) await bob.screenshot({ path: `${shots}/invite-accept.png` });
  await bob.getByRole("button", { name: "Create your account" }).click();
  await expect(bob).toHaveURL(/\/kopi-kenangan$/);
  // A member sees the members list but cannot invite or edit settings.
  await bob.getByRole("button", { name: "Switch workspace" }).click();
  await expect(bob.locator(".ws-menu-current")).toContainText("Member");
  await bob.getByRole("menuitem", { name: "Members" }).click();
  await expect(bob.getByRole("button", { name: "Invite people" })).toHaveCount(0);
  await bob.getByRole("link", { name: "Settings" }).click();
  await expect(bob.getByLabel("Workspace name")).toBeDisabled();
  await bob.context().close();
});

test("owner changes Bob's role, then removes him", async ({ page, hub }) => {
  await login(page, hub);
  await expect(page).toHaveURL(/\/kopi-kenangan$/);
  await page.getByRole("button", { name: "Switch workspace" }).click();
  await page.getByRole("menuitem", { name: "Invite members" }).click();
  const bobRow = page.locator(".row", { hasText: "bob@example.com" });
  await bobRow.getByLabel("Role").selectOption("guest");
  await expect(bobRow.getByLabel("Role")).toHaveValue("guest");
  await bobRow.getByRole("button", { name: "Remove" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Remove" }).click();
  await expect(page.locator(".row", { hasText: "bob@example.com" })).toHaveCount(0);

  // Workspace switcher and user menu.
  await page.locator(".ws-switch").click();
  await expect(page.getByRole("menuitem", { name: /Create workspace/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Your account" }).click();
  await page.getByRole("menuitem", { name: "Log out" }).click();
  await expect(page).toHaveURL(/\/login$/);
});

test("delete to trash and restore from profile", async ({ page, hub }) => {
  await login(page, hub);
  await page.getByRole("button", { name: "Switch workspace" }).click();
  await page.getByRole("menuitem", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Delete workspace" }).click();
  await page.getByLabel("Type kopi-kenangan to confirm").fill("kopi-kenangan");
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await expect(page).toHaveURL(/\/create-workspace$/);

  await page.goto(hub.url + "/profile");
  await page.getByRole("button", { name: "Restore" }).click();
  await expect(page.getByText("Nothing in the trash.")).toBeVisible();
  await page.goto(hub.url + "/");
  await expect(page).toHaveURL(/\/kopi-kenangan$/);
});

test("instance admin creates a reset link", async ({ page, hub }) => {
  await login(page, hub);
  await page.goto(hub.url + "/admin");
  const row = page.locator(".row", { hasText: "bob@example.com" });
  await row.getByRole("button", { name: "Reset link" }).click();
  await expect(page.getByRole("dialog").locator("code")).toContainText("/reset/");
  if (shots) await page.screenshot({ path: `${shots}/admin.png` });
});
