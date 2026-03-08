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

  await page.addInitScript(() => {
    ;(
      window as typeof window & {
        __GTO_TEST_SCENARIO__?: string
        __GTO_FORCE_ACTION_DELAY_MS__?: number
      }
    ).__GTO_TEST_SCENARIO__ = 'flopRevealThenAction'
    ;(
      window as typeof window & {
        __GTO_TEST_SCENARIO__?: string
        __GTO_FORCE_ACTION_DELAY_MS__?: number
      }
    ).__GTO_FORCE_ACTION_DELAY_MS__ = 900
  })
  await page.goto('/')
  await expect(page.getByRole('button', { name: /Call|Check/ })).toBeVisible()
  await expect(page.getByRole('button', { name: 'New match' })).toBeEnabled()
  await expect(
    page.getByRole('heading', { name: "Heads-Up Hold'em" }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await capture(page, '01-opening-hand.png')
  await page.getByLabel('Action tray').getByRole('button', { name: 'Call', exact: true }).click()
  await expect(
    page.getByLabel('Board cards').getByRole('img', { name: /of/i }),
  ).toHaveCount(3, { timeout: 5_000 })
  await expect(page.locator('.action-bubble')).toContainText('Thinking')
  await capture(page, '02-bot-thinking.png')

  await expect(page.getByLabel('Bot panel')).not.toContainText('Thinking')
  await expect(page.locator('.action-bubble')).toHaveCount(1)
  await capture(page, '03-bot-action.png')

  await page.getByLabel('Action tray').getByRole('button', { name: 'Call', exact: true }).click()
  await expect(page.getByRole('button', { name: 'Deal next hand' })).toBeVisible()
  await expect(
    page.getByLabel('Board cards').getByRole('img', { name: /of/i }),
  ).toHaveCount(5, { timeout: 5_000 })
  await capture(page, '04-terminal-hand.png')

  await page.evaluate(() => {
    delete (window as typeof window & { __GTO_TEST_SCENARIO__?: string }).__GTO_TEST_SCENARIO__
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
