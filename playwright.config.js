import { defineConfig } from "@playwright/test";

/**
 * Runs the frontend against a plain static server with a stubbed Tauri bridge.
 *
 * The app has no build step, so this is the whole test environment: no bundler,
 * no Rust toolchain, no dev.ps1. A CI job on ubuntu-latest can run it as-is.
 */
export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1280, height: 900 },
  },
  webServer: {
    command: "node scripts/serve.mjs src 4173",
    url: "http://127.0.0.1:4173/index.html",
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
  },
});
