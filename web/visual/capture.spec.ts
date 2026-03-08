import { mkdir, readdir, rm } from 'node:fs/promises'
import path from 'node:path'

import { expect, test, type Page } from '@playwright/test'

const SCREENSHOT_DIR = path.resolve(process.cwd(), 'artifacts', 'screenshots')

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    ;(window as typeof window & { __GTO_TEST_SEED__?: number }).__GTO_TEST_SEED__ = 7
  })
})

test('capture stable desktop screenshots for visual review', async ({ page }) => {
  await mkdir(SCREENSHOT_DIR, { recursive: true })
  await clearExistingScreenshots()

  await page.goto('/')
  await expect(page.getByRole('button', { name: /Call|Check/ })).toBeVisible()
  await expect(page.getByRole('button', { name: 'New match' })).toBeEnabled()
  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_TEST_SEED__?: number }).__GTO_TEST_SEED__ = 0
  })
  await page.getByRole('button', { name: 'New match' }).click()
  await expect(
    page.getByRole('heading', { name: "Heads-Up Hold'em" }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await capture(page, '01-opening-hand.png')

  await page.getByLabel('Action tray').getByRole('button', { name: 'Call', exact: true }).click()
  await expect(
    page.getByLabel('Board cards').getByRole('img', { name: /of/i }),
  ).toHaveCount(3, { timeout: 5_000 })
  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_ACTION_DELAY_MS__?: number }).__GTO_FORCE_ACTION_DELAY_MS__ =
      280
  })
  await page.getByLabel('Action tray').getByRole('button', { name: /Raise to 5.0/i }).click()
  await expect(page.locator('.action-bubble')).toContainText('Thinking')
  await capture(page, '02-bot-thinking.png')

  await expect(page.getByLabel('Bot panel')).not.toContainText('Thinking')
  await expect(page.locator('.action-bubble')).toHaveCount(1)
  await capture(page, '03-bot-action.png')

  for (let step = 0; step < 32; step += 1) {
    if (!(await clickPreferredAction(page))) {
      break
    }
  }

  await expect(page.getByRole('button', { name: 'Deal next hand' })).toBeVisible()
  await capture(page, '04-terminal-hand.png')

  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_WORKER_ERROR__?: string }).__GTO_FORCE_WORKER_ERROR__ =
      'forced initialization failure for screenshot capture'
  })
  await page.getByRole('button', { name: 'New match' }).click()
  await expect(page.getByRole('alert')).toContainText(
    'forced initialization failure for screenshot capture',
  )
  await capture(page, '05-worker-error.png')
})

async function capture(page: Page, filename: string): Promise<void> {
  await page.screenshot({
    path: path.join(SCREENSHOT_DIR, filename),
    fullPage: true,
  })
}

async function clearExistingScreenshots(): Promise<void> {
  const entries = await readdir(SCREENSHOT_DIR, { withFileTypes: true })
  await Promise.all(
    entries
      .filter((entry) => entry.isFile() && entry.name.endsWith('.png'))
      .map((entry) => rm(path.join(SCREENSHOT_DIR, entry.name))),
  )
}

async function clickPreferredAction(page: Page): Promise<boolean> {
  const actionTray = page.getByLabel('Action tray')

  for (let attempt = 0; attempt < 120; attempt += 1) {
    const nextHandButton = actionTray.getByRole('button', { name: 'Deal next hand' })
    if (await nextHandButton.isVisible().catch(() => false)) {
      return false
    }

    const labels = await actionTray.locator('button:not([disabled])').evaluateAll((buttons) =>
      buttons
        .map((button) => button.textContent?.trim() ?? '')
        .filter((value) => value.length > 0),
    )
    const label =
      labels.find((value) => value === 'Check' || value === 'Call') ?? labels[0]

    if (label) {
      await actionTray.getByRole('button', { name: label, exact: true }).click()
      return true
    }

    await page.waitForTimeout(100)
  }

  throw new Error('No enabled action buttons were available for screenshot capture')
}
