import { mkdir } from 'node:fs/promises'
import path from 'node:path'

import { expect, test, type Page } from '@playwright/test'

const SCREENSHOT_DIR = path.resolve(
  process.cwd(),
  'artifacts',
  'screenshots',
)

test('capture stable desktop screenshots for visual review', async ({ page }) => {
  await mkdir(SCREENSHOT_DIR, { recursive: true })

  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'GTO Poker' })).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await capture(page, '01-opening-hand.png')

  await page.getByRole('button', { name: /Hybrid Play/i }).click()
  await page.getByRole('button', { name: 'Restart session' }).click()
  await expect(page.getByLabel('Session activity')).toContainText('Hybrid Play')
  await capture(page, '02-hybrid-play.png')

  for (let step = 0; step < 16; step += 1) {
    if (!(await clickPreferredAction(page))) {
      break
    }
  }

  await expect(page.getByLabel('Hand status')).toContainText('terminal')
  await capture(page, '03-terminal-hand.png')

  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_WORKER_ERROR__?: string }).__GTO_FORCE_WORKER_ERROR__ =
      'forced initialization failure for screenshot capture'
  })
  await page.getByRole('button', { name: 'Restart session' }).click()
  await expect(page.getByRole('alert')).toContainText(
    'forced initialization failure for screenshot capture',
  )
  await capture(page, '04-worker-error.png')
})

async function capture(page: Page, filename: string): Promise<void> {
  await page.screenshot({
    path: path.join(SCREENSHOT_DIR, filename),
    fullPage: true,
  })
}

async function clickPreferredAction(page: Page): Promise<boolean> {
  const activity = page.getByLabel('Session activity')
  const actions = page.getByLabel('Available actions')
  await expect(activity).toContainText('Ready', { timeout: 20_000 })

  const completeMarker = actions.getByText('This hand is complete.')
  if (await completeMarker.isVisible().catch(() => false)) {
    return false
  }

  const labels = await actions.locator('button').evaluateAll((buttons) =>
    buttons
      .filter((button) => !(button as HTMLButtonElement).disabled)
      .map((button) => button.textContent?.trim() ?? '')
      .filter((value) => value.length > 0),
  )

  const label =
    labels.find((value) => value === 'Check' || value === 'Call') ?? labels[0]
  if (!label) {
    if (await completeMarker.isVisible().catch(() => false)) {
      return false
    }
    throw new Error('No enabled action buttons were available for screenshot capture')
  }

  await actions.getByRole('button', { name: label, exact: true }).click()
  return true
}
