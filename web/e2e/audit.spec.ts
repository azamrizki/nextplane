import type { Locator, Page } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { ADMIN, expect, login, test } from "./fixtures";

// UI audit: opens every menu, picker and popup in the places they live (board columns, list
// rows, spreadsheet cells, the peek, dialogs, the sidebar, widgets…) at two window sizes and
// checks each is fully on screen and not cut off or covered (elementFromPoint on a grid of
// points). Runs only when NEXTPLANE_AUDIT names an output folder; writes report.json and a
// screenshot of every finding.
const out = process.env.NEXTPLANE_AUDIT;
test.skip(!out, "set NEXTPLANE_AUDIT to an output folder to audit the UI");
test.describe.configure({ mode: "serial" });

let base = "";
let projectUrl = "";
let itemId = "";
let boardPath = "";

interface Finding {
  scenario: string;
  size: string;
  layers: { cls: string; rect: number[]; inView: boolean; hidden: (string | number | null)[][] }[];
  error?: string;
}

test("seed", async ({ page, hub }) => {
  await page.goto(hub.setupLink);
  await page.getByLabel("Instance name").fill("Acme");
  await page.getByLabel("Your name").fill("Ana Putri");
  await page.getByLabel("Email").fill(ADMIN.email);
  await page.getByLabel("Password").fill(ADMIN.password);
  await page.getByRole("button", { name: "Create admin and continue" }).click();
  await page.getByLabel("Workspace name").fill("Acme");
  await page.getByRole("button", { name: "Create workspace" }).click();
  await expect(page).toHaveURL(/\/acme$/);
  const r = await page.evaluate(async () => {
    const h = { "Content-Type": "application/json" };
    const j = async (url: string, body: unknown, method = "POST") => {
      const res = await fetch(url, { method, headers: h, body: JSON.stringify(body) });
      return res.status === 204 ? null : res.json();
    };
    const me = await (await fetch("/api/v1/me")).json();
    const ws = "/api/v1/workspaces/acme";
    const p = await j(`${ws}/projects`, { name: "Website" });
    for (const n of ["Mobile App", "Data Platform", "Marketing Site", "Billing", "Support Portal", "Design System", "Infra"]) await j(`${ws}/projects`, { name: n });
    const pp = `${ws}/projects/${p.id}`;
    await j(pp, { feature_cycles: true, feature_modules: true, feature_pages: true, feature_intake: true, estimate_type: "points", estimate_values: ["1", "2", "3", "5", "8"] }, "PATCH");
    for (const n of ["Bug", "Feature", "UX", "Backend", "Frontend", "Docs", "Security", "Performance"]) await j(`${pp}/labels`, { name: n });
    for (const n of ["Onboarding", "Payments", "Search", "Mobile"]) await j(`${pp}/modules`, { name: n });
    let first = "";
    for (let i = 0; i < 14; i++) {
      const w = await j(`${pp}/work-items`, { title: `Work item number ${i + 1} with a longer title`, assignee_ids: i % 2 ? [me.id] : [] });
      first ||= w.id;
    }
    await j(`${pp}/work-items/${first}/comments`, { body_html: "<p>First comment</p>" });
    await j(`${ws}/quick-links`, { url: "github.com/acme/website", title: "Repository" });
    for (let i = 0; i < 9; i++) await j(`${ws}/stickies`, { description_html: `<p>Sticky ${i + 1}</p>` });
    await j(`${ws}/views`, { name: "Urgent work", config: { filters: { priority: ["urgent"] } } });
    const pub = await j(`${pp}/publish`, { enabled: true, comments: true, reactions: true, votes: true });
    return { pid: p.id as string, first, board: String(pub.path) };
  });
  base = `${hub.url}/acme`;
  projectUrl = `${base}/projects/${r.pid}`;
  itemId = r.first;
  boardPath = r.board;
});

/** Visible floating layers and how much of each is really visible. */
async function measure(page: Page) {
  return page.evaluate(() => {
    const layers = [...document.querySelectorAll<HTMLElement>(".floating, .popover, .mention-menu, .ref-card")].filter((e) => {
      const r = e.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && getComputedStyle(e).visibility !== "hidden" && !e.parentElement?.closest(".floating, .popover");
    });
    return layers.map((e) => {
      const r = e.getBoundingClientRect();
      const inView = r.left >= -0.5 && r.top >= -0.5 && r.right <= innerWidth + 0.5 && r.bottom <= innerHeight + 0.5;
      const hidden: (string | number | null)[][] = [];
      for (const fx of [0.08, 0.5, 0.92])
        for (const fy of [0.08, 0.5, 0.92]) {
          const x = r.left + r.width * fx;
          const y = r.top + r.height * fy;
          if (x < 0 || y < 0 || x > innerWidth || y > innerHeight) {
            hidden.push([Math.round(x), Math.round(y), "off screen"]);
            continue;
          }
          const hit = document.elementFromPoint(x, y);
          if (!hit || !e.contains(hit)) hidden.push([Math.round(x), Math.round(y), hit ? `${hit.tagName.toLowerCase()}.${String(hit.className).slice(0, 40)}` : null]);
        }
      return { cls: String(e.className), rect: [r.left, r.top, r.width, r.height].map(Math.round), inView, hidden };
    });
  });
}

const SIZES = [
  { name: "1280x800", width: 1280, height: 800 },
  { name: "1024x600", width: 1024, height: 600 },
];

for (const size of SIZES) {
  test(`audit at ${size.name}`, async ({ page, hub }) => {
    test.setTimeout(300_000);
    // A trigger that is not there should fail fast, not wait for the test timeout.
    page.setDefaultTimeout(5_000);
    await page.setViewportSize({ width: size.width, height: size.height });
    await login(page, hub);
    mkdirSync(`${out}`, { recursive: true });
    const findings: Finding[] = [];
    let n = 0;
    const check = async (scenario: string, open: () => Promise<unknown>) => {
      n++;
      try {
        await open();
        await page.waitForTimeout(250);
        const layers = await measure(page);
        if (process.env.NEXTPLANE_AUDIT_ALL) await page.screenshot({ path: `${out}/all-${size.name}-${String(n).padStart(2, "0")}-${scenario.replace(/[^a-z0-9]+/gi, "-")}.png` });
        if (!layers.length) {
          findings.push({ scenario, size: size.name, layers, error: "nothing opened" });
        } else if (layers.some((l) => !l.inView || l.hidden.length)) {
          findings.push({ scenario, size: size.name, layers });
          await page.screenshot({ path: `${out}/${size.name}-${String(n).padStart(2, "0")}-${scenario.replace(/[^a-z0-9]+/gi, "-")}.png` });
        }
      } catch (e) {
        findings.push({ scenario, size: size.name, layers: [], error: String(e).split("\n")[0] });
      }
      await page.keyboard.press("Escape");
      await page.keyboard.press("Escape");
      await page.waitForTimeout(100);
    };
    const last = (l: Locator) => l.last();

    // Board: the case reported (a card's state picker inside a short column), and the card menu.
    await page.goto(projectUrl);
    await page.getByRole("tab", { name: "Board" }).click();
    await page.waitForTimeout(300);
    const card = last(page.locator(".board-card"));
    await check("board card state picker", async () => (await card.scrollIntoViewIfNeeded(), card.getByRole("button", { name: "State" }).click()));
    await check("board card menu", async () => (await card.hover(), card.getByRole("button", { name: "Work item actions" }).click()));
    const firstCard = page.locator(".board-card").first();
    await check("board first card labels", async () => firstCard.getByRole("button", { name: "Labels" }).click());
    // Our own date picker (v0.3) from the last card's due date.
    await check("board card due date picker", async () => (await card.scrollIntoViewIfNeeded(), card.getByRole("button", { name: "Due date" }).click()));

    // List rows (chips row clips, last rows near the bottom).
    await page.getByRole("tab", { name: "List" }).click();
    await page.waitForTimeout(300);
    const row = last(page.locator(".list-row"));
    await check("list last row priority", async () => (await row.scrollIntoViewIfNeeded(), row.getByRole("button", { name: "Priority" }).click()));
    await check("list last row assignees", async () => row.getByRole("button", { name: "Assignees" }).click());
    await check("list last row menu", async () => (await row.hover(), row.getByRole("button", { name: "Work item actions" }).click()));
    await check("display menu", async () => page.getByRole("button", { name: "Display" }).click());
    await check("filters add menu", async () => {
      if (!(await page.locator(".filter-row").count())) await page.getByRole("button", { name: /^Filters/ }).click();
      await page.getByRole("button", { name: "Add filter" }).click();
    });
    await check("saved views menu", async () => page.locator(".page-header").getByRole("button", { name: /views/i }).first().click());
    await check("project switcher", async () => page.getByRole("button", { name: "Switch project" }).click());

    // Spreadsheet cells.
    await page.getByRole("tab", { name: "Spreadsheet" }).click();
    await page.waitForTimeout(300);
    const srow = last(page.locator(".sheet-row:not(.sheet-head)"));
    await check("sheet last row state", async () => (await srow.scrollIntoViewIfNeeded(), srow.getByRole("button", { name: "State" }).click()));
    await check("sheet last row modules", async () => srow.getByRole("button", { name: "Modules" }).click());
    await page.getByRole("tab", { name: "List" }).click();

    // Peek and the full page.
    // Esc closes the peek too, so each check opens it again.
    const peek = page.locator(".peek");
    const openPeek = async () => {
      await page.goto(`${projectUrl}?peek=${itemId}`);
      await peek.waitFor();
      await page.waitForTimeout(300);
    };
    await check("peek actions menu", async () => (await openPeek(), peek.getByRole("button", { name: "Work item actions" }).click()));
    await check("peek labels", async () => (await openPeek(), last(peek.getByRole("button", { name: "Labels" })).click()));
    await check("peek modules", async () => (await openPeek(), last(peek.getByRole("button", { name: "Modules" })).click()));
    await check("peek add reaction", async () => {
      await openPeek();
      const b = last(peek.getByRole("button", { name: "Add reaction" }));
      await b.scrollIntoViewIfNeeded();
      await b.click();
    });
    await check("peek comment menu", async () => {
      await openPeek();
      const b = last(peek.getByRole("button", { name: "Comment actions" }));
      await b.scrollIntoViewIfNeeded();
      await b.click();
    });
    await check("peek comment mention", async () => {
      await openPeek();
      const box = last(peek.getByRole("textbox", { name: /comment/i }));
      await box.scrollIntoViewIfNeeded();
      await box.click();
      await page.keyboard.type("@");
    });
    await page.goto(`${projectUrl}/work-items/${itemId}`);
    await page.waitForTimeout(500);
    await check("detail side cycle", async () => last(page.locator(".wi-side").getByRole("button", { name: "Cycle" })).click());
    await check("detail side modules", async () => last(page.locator(".wi-side").getByRole("button", { name: "Modules" })).click());

    // The create dialog (a modal in the top layer).
    await page.goto(projectUrl);
    await check("create dialog modules", async () => {
      await page.getByRole("button", { name: "Add work item", exact: true }).click();
      await page.getByRole("dialog").getByRole("button", { name: "Modules" }).click();
    });
    await check("create dialog labels", async () => {
      if (!(await page.getByRole("dialog").count())) await page.getByRole("button", { name: "Add work item", exact: true }).click();
      await page.getByRole("dialog").getByRole("button", { name: "Labels" }).click();
    });
    await check("create dialog due date", async () => {
      if (!(await page.getByRole("dialog").count())) await page.getByRole("button", { name: "Add work item", exact: true }).click();
      await page.getByRole("dialog").getByRole("button", { name: "Due date" }).click();
    });
    // Cycle dates: the two-month range picker inside a dialog.
    await page.goto(`${projectUrl}/cycles`);
    await check("cycle dialog range picker", async () => {
      await page.getByRole("button", { name: /new cycle/i }).first().click();
      await page.getByRole("dialog").getByLabel("Start date").click();
    });
    // Settings: a Select near the bottom of the form, and a state colour.
    await page.goto(`${projectUrl}/settings`);
    await check("settings lead select", async () => page.getByLabel("Lead", { exact: true }).click());
    await page.goto(`${projectUrl}/settings/states`);
    await check("settings state colour", async () => last(page.getByRole("button", { name: "State colour" })).click());
    await page.keyboard.press("Escape");

    // Shell: sidebar project menu (in a scrolling sidebar), switcher, help, account.
    await check("sidebar last project menu", async () => {
      const proj = last(page.locator(".sidebar .side-project"));
      await proj.scrollIntoViewIfNeeded();
      await proj.hover();
      await proj.getByRole("button", { name: /actions$/ }).click();
    });
    await check("workspace switcher", async () => page.getByRole("button", { name: "Switch workspace" }).click());
    await check("help menu", async () => page.getByRole("button", { name: "Help" }).click());
    await check("account menu", async () => page.getByRole("button", { name: "Your account" }).click());

    // Home widgets and stickies.
    await page.goto(base);
    await page.waitForTimeout(500);
    await check("quick link menu", async () => {
      const c = page.locator(".link-card").first();
      await c.hover();
      await c.getByRole("button", { name: /^Actions for/ }).click();
    });
    await check("recents filter", async () => page.getByRole("button", { name: /^Show:/ }).click());
    await check("home sticky colour", async () => {
      const s = last(page.locator(".sticky-grid .sticky"));
      await s.scrollIntoViewIfNeeded();
      await s.getByRole("button", { name: "Colour" }).click();
    });
    await page.goto(`${base}/stickies`);
    await page.waitForTimeout(500);
    await check("stickies page colour", async () => last(page.locator(".sticky")).getByRole("button", { name: "Colour" }).click());

    // Workspace views, archives, exports, inbox.
    await page.goto(`${base}/views/all`);
    await page.waitForTimeout(500);
    const vrow = last(page.locator(".sheet-row:not(.sheet-head)"));
    await check("workspace view last row state", async () => (await vrow.scrollIntoViewIfNeeded(), vrow.getByRole("button", { name: "State" }).click()));
    await page.locator(".view-tabs").getByRole("link", { name: "Urgent work" }).click();
    await check("workspace view menu", async () => page.getByRole("button", { name: "View actions" }).click());
    await page.goto(`${base}/settings/exports`);
    await check("export dialog projects", async () => {
      await page.getByRole("button", { name: "Export" }).first().click();
      await page.getByRole("dialog").getByRole("button", { name: "Projects" }).click();
    });
    await page.keyboard.press("Escape");

    // The public board's item dialog.
    await page.goto(`${hub.url}${boardPath}`);
    await page.getByText("Work item number 1 with a longer title", { exact: true }).click();
    await check("board item react", async () => page.getByRole("dialog").getByRole("button", { name: "React" }).first().click());

    // Pages whose content is wider than the window or than the main panel (cut off, or a
    // horizontal scrollbar where none is meant to be). Wide layouts that scroll on purpose
    // (board, spreadsheet, timeline) are left out.
    const pages = [
      ["home", base],
      ["projects", `${base}/projects`],
      ["list", projectUrl],
      ["views list", `${base}/views`],
      ["profile", `${base}/profile/me`],
      ["stickies", `${base}/stickies`],
      ["inbox", `${base}/inbox`],
      ["analytics", `${base}/analytics`],
      ["drafts", `${base}/drafts`],
      ["archives", `${projectUrl}/archives`],
      ["cycles", `${projectUrl}/cycles`],
      ["modules", `${projectUrl}/modules`],
      ["pages", `${projectUrl}/pages`],
      ["intake", `${projectUrl}/intake`],
      ["project analytics", `${projectUrl}/analytics`],
      ["project settings", `${projectUrl}/settings`],
      ["automations", `${projectUrl}/settings/automations`],
      ["publish", `${projectUrl}/settings/publish`],
      ["workspace settings", `${base}/settings`],
      ["members", `${base}/settings/members`],
      ["exports", `${base}/settings/exports`],
      ["webhooks", `${base}/settings/webhooks`],
      ["admin", `${hub.url}/admin`],
      ["account", `${hub.url}/profile`],
      ["public board", `${hub.url}${boardPath}`],
    ] as const;
    const me = await page.evaluate(async () => (await (await fetch("/api/v1/me")).json()).id as string);
    for (const [name, url] of pages) {
      n++;
      await page.goto(url.replace("/profile/me", `/profile/${me}`));
      await page.waitForTimeout(600);
      const over = await page.evaluate(() => {
        const doc = document.documentElement;
        const main = document.querySelector<HTMLElement>(".app-main");
        const res: string[] = [];
        if (doc.scrollWidth > innerWidth + 1) res.push(`page ${doc.scrollWidth} > ${innerWidth}`);
        if (main && main.scrollWidth > main.clientWidth + 1) res.push(`main ${main.scrollWidth} > ${main.clientWidth}`);
        // The widest culprits, to find them quickly.
        if (res.length) {
          const limit = main ? main.getBoundingClientRect().right : innerWidth;
          const wide = [...document.querySelectorAll<HTMLElement>("body *")]
            .filter((e) => e.getBoundingClientRect().right > limit + 1 && getComputedStyle(e).position !== "fixed")
            .slice(0, 5)
            .map((e) => `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`);
          res.push(...wide);
        }
        return res;
      });
      if (over.length) {
        findings.push({ scenario: `page overflow: ${name}`, size: size.name, layers: [], error: over.join(" | ") });
        await page.screenshot({ path: `${out}/${size.name}-${String(n).padStart(2, "0")}-overflow-${name.replace(/[^a-z0-9]+/gi, "-")}.png` });
      }
    }

    writeFileSync(`${out}/report-${size.name}.json`, JSON.stringify({ checked: n, findings }, null, 2));
    console.log(`audit ${size.name}: ${n} checked, ${findings.length} findings`);
    for (const f of findings) console.log(` - ${f.scenario}: ${f.error ?? JSON.stringify(f.layers.map((l) => ({ cls: l.cls, inView: l.inView, hidden: l.hidden.length })))}`);
  });
}
