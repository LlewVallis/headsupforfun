import { expect, test, type Page } from '@playwright/test'

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    ;(window as typeof window & { __GTO_TEST_SEED__?: number }).__GTO_TEST_SEED__ = 7
  })
})

test('plays a complete browser hand and deals the next one', async ({ page }) => {
  await page.goto('/')

  await expect(
    page.getByRole('heading', { name: "Heads-Up Hold'em" }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await expect(page.getByLabel('Action tray')).not.toContainText('hybrid play mode')
  await expect(page.getByLabel('Hero panel')).toContainText('You')
  await expect(page.getByLabel('Bot panel')).toContainText('Solver Bot')
  await expect(page.getByText('Hand 1')).toBeVisible()
  await expect(page.getByText('Seed')).toHaveCount(0)

  for (let step = 0; step < 32; step += 1) {
    if (!(await clickPreferredAction(page))) {
      break
    }
  }

  await expect(page.getByRole('button', { name: 'Deal next hand' })).toBeVisible()
  await expect(page.getByLabel('Bot panel').getByRole('img', { name: /of/i })).toHaveCount(2)

  await page.getByRole('button', { name: 'Deal next hand' }).click()

  await expect(page.getByText('Hand 2')).toBeVisible()
  await expect(page.getByRole('button', { name: /Call|Check/ })).toBeVisible()
})

test('shows a recoverable error when the table fails to initialize', async ({ page }) => {
  await page.goto('/')

  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_WORKER_ERROR__?: string }).__GTO_FORCE_WORKER_ERROR__ =
      'forced initialization failure for e2e'
  })

  await page.getByRole('button', { name: 'New match' }).click()

  await expect(page.getByRole('alert')).toContainText('forced initialization failure for e2e')
  await page.getByRole('button', { name: 'Reload table' }).click()

  await expect(page.getByRole('alert')).toHaveCount(0)
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await expect(page.getByRole('button', { name: /Call|Check/ })).toBeVisible()
})

test('keeps the player-facing app on the fixed hybrid-play experience', async ({ page }) => {
  await page.goto('/')

  await expect(page.getByText('Hybrid Play')).toHaveCount(0)
  await expect(page.getByLabel('Action tray')).not.toContainText('hybrid play mode')
  await expect(page.getByRole('button', { name: 'New match' })).toBeVisible()
})

test('shows bot thinking feedback and then fades the action bubble', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('button', { name: /Call|Check/ })).toBeVisible()

  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_ACTION_DELAY_MS__?: number }).__GTO_FORCE_ACTION_DELAY_MS__ =
      220
  })

  const firstAction = page
    .getByLabel('Action tray')
    .getByRole('button', { name: /Call|Check/ })
    .first()
  await firstAction.click()

  await expect(page.locator('.action-bubble')).toContainText('Thinking')
  await expect(page.locator('.action-bubble')).toHaveCount(1)

  await expect(page.locator('.action-bubble')).not.toContainText('Thinking', { timeout: 5_000 })
  await expect(page.locator('.action-bubble')).toHaveCount(1)
  await expect(page.locator('.action-bubble')).toHaveCount(0, { timeout: 5_000 })
})

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

  throw new Error('No enabled action buttons became available in time')
}
