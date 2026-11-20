import { defineConfig } from "@playwright/test";

// Browser tests against a throwaway server: every test file gets its own binary, port and
// temp data folder (see fixtures.ts). Never the real ~/.nextplane.
export default defineConfig({
  testDir: ".",
  fullyParallel: false, // tests in a file share one server and run in order
  workers: 2,
  retries: 0,
  reporter: [["list"]],
  use: {
    headless: true,
    viewport: { width: 1280, height: 800 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  outputDir: process.env.NEXTPLANE_E2E_OUT ?? "../../target/e2e-results",
});
