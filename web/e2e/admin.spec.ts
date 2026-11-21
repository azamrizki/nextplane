import { createServer, type Server } from "node:http";
import { ADMIN, expect, test } from "./fixtures";

/** A stand-in for an OpenAI-compatible API, answering every chat request the same way. */
async function fakeAi(): Promise<{ url: string; server: Server; asked: string[] }> {
  const asked: string[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      asked.push(body);
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify({ choices: [{ message: { role: "assistant", content: "The login page fails on Safari.\n\nDone when it works in every browser." } }] }));
    });
  });
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as { port: number }).port;
  return { url: `http://127.0.0.1:${port}/v1`, server, asked };
}

const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==", "base64");

test("instance admins set up sign-in providers, the AI assistant and covers", async ({ page, hub, browser }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);

  // Sign-in methods: set up GitHub, turn it on, and the login page offers it.
  await page.goto(`${hub.url}/admin`);
  const github = page.locator(".row", { hasText: "GitHub" });
  await github.getByRole("button", { name: "Set up" }).click();
  const dlg = page.getByRole("dialog");
  await expect(dlg.locator(".link-box")).toContainText("/api/v1/auth/oauth/github/callback");
  await dlg.getByLabel("Client ID").fill("gh-client");
  await dlg.getByLabel("Client secret").fill("gh-secret");
  await dlg.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".toast")).toContainText("GitHub is set up");
  await github.getByRole("switch", { name: "Sign in with GitHub" }).check();
  await expect(github.getByRole("switch", { name: "Sign in with GitHub" })).toBeChecked();
  const visitor = await (await browser.newContext()).newPage();
  await visitor.goto(`${hub.url}/login`);
  await expect(visitor.getByRole("link", { name: "Continue with GitHub" })).toHaveAttribute("href", /\/api\/v1\/auth\/oauth\/github\/start/);
  await visitor.context().close();

  // The AI assistant, on a stand-in service.
  const ai = await fakeAi();
  try {
    await page.getByLabel("API key").fill("sk-test");
    await page.getByLabel("API address").fill(ai.url);
    await page.locator(".card", { hasText: "AI writing assistant" }).getByRole("button", { name: "Save" }).first().click();
    await expect(page.getByText("On for every workspace")).toBeVisible();

    await page.goto(`${hub.url}/acme`);
    await page.evaluate(async () => {
      await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ name: "Website" }) });
    });
    await page.reload();
    await page.locator(".sidebar").getByRole("button", { name: "New work item" }).click();
    const create = page.getByRole("dialog");
    await create.getByLabel("Title").fill("Fix the login page");
    await create.getByRole("button", { name: "Draft from title" }).click();
    await expect(create.getByRole("textbox", { name: "Description" })).toContainText("fails on Safari");
    expect(ai.asked.join("")).toContain("Fix the login page");
    await create.getByRole("button", { name: "Ask AI" }).click();
    await page.getByRole("textbox", { name: "Ask AI" }).fill("List the steps to reproduce");
    await page.getByRole("button", { name: "Generate" }).click();
    await expect(page.locator(".ai-answer")).toContainText("Done when it works");
  } finally {
    ai.server.close();
  }

  // An uploaded cover shows on the project's card.
  await page.keyboard.press("Escape");
  const pid = await page.evaluate(async () => (await (await fetch("/api/v1/workspaces/acme/projects")).json())[0].id as string);
  await page.goto(`${hub.url}/acme/projects/${pid}/settings`);
  await page.getByLabel("Upload image").setInputFiles({ name: "cover.png", mimeType: "image/png", buffer: PNG });
  await expect(page.locator(".cover-preview")).toHaveAttribute("style", /api\/v1\/files\//);
  await page.goto(`${hub.url}/acme/projects`);
  await expect(page.locator(".proj-card-cover.has-image")).toHaveCount(1);
});
