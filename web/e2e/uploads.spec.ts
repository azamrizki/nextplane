import { ADMIN, expect, test } from "./fixtures";

// A 4×4 red PNG.
const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEUlEQVR4nGP4z8CAhBgGlgMAcd0P8a8ZRGYAAAAASUVORK5CYII=";

test("paste an image into a comment and preview an attached image", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const url = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const p = await (await fetch("/api/v1/workspaces/acme/projects", { method: "POST", headers: h, body: JSON.stringify({ name: "Website" }) })).json();
    const w = await (await fetch(`/api/v1/workspaces/acme/projects/${p.id}/work-items`, { method: "POST", headers: h, body: JSON.stringify({ title: "Broken banner" }) })).json();
    return `/acme/projects/${p.id}/work-items/${w.id}`;
  });
  await page.goto(hub.url + url);

  // Paste: build a clipboard event with a PNG file, as the browser does.
  const box = page.locator(".comment-box").getByRole("textbox");
  await box.click();
  await box.pressSequentially("Screenshot: ");
  await page.evaluate((b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const dt = new DataTransfer();
    dt.items.add(new File([bytes], "shot.png", { type: "image/png" }));
    document.querySelector(".comment-box .ProseMirror")!.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
  }, PNG);
  await expect(page.locator(".comment-box img")).toHaveAttribute("src", /\/api\/v1\/files\//);
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  const img = page.locator(".comment img").first();
  await expect(img).toBeVisible();
  expect(await img.evaluate((i: HTMLImageElement) => i.complete && i.naturalWidth)).toBe(4);

  // Attach the same image as a file and open its preview.
  await page.locator('input[type="file"][multiple]').setInputFiles({ name: "banner.png", mimeType: "image/png", buffer: Buffer.from(PNG, "base64") });
  await page.getByRole("button", { name: "Preview: banner.png" }).click();
  const dlg = page.getByRole("dialog", { name: "banner.png" });
  await expect(dlg.locator("img")).toBeVisible();
  await expect(dlg.getByRole("link", { name: /Download/ })).toHaveAttribute("download", "banner.png");
});
