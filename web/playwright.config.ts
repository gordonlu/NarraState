import { defineConfig } from '@playwright/test'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const externalBaseUrl = process.env.E2E_BASE_URL
const webRoot = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(webRoot, '..')

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  reporter: 'line',
  use: {
    baseURL: externalBaseUrl ?? 'http://127.0.0.1:3100',
    channel: 'chrome',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: externalBaseUrl
    ? undefined
    : {
        command:
          `cargo run --locked --manifest-path "${repoRoot}/Cargo.toml" -p narrastate-server -- serve --port 3100 --db "${repoRoot}/.narrastate-e2e.db" --cases "${repoRoot}/cases" --web "${webRoot}/dist"`,
        url: 'http://127.0.0.1:3100/api/v1/health',
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
})
