import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './visual',
  fullyParallel: false,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://127.0.0.1:4273',
    trace: 'off',
  },
  webServer: {
    command: 'npm run dev:no-wasm -- --host 127.0.0.1 --port 4273 --strictPort',
    url: 'http://127.0.0.1:4273',
    reuseExistingServer: true,
    timeout: 30_000,
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1440, height: 1280 },
      },
    },
  ],
})
