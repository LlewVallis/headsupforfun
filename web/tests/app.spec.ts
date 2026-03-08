import { expect, test, type Page } from '@playwright/test'

test('plays a complete seeded browser hand and deals the next one', async ({
  page,
}) => {
  await page.goto('/')

  await expect(
    page.getByRole('heading', { name: 'GTO Poker' }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await expect(page.getByLabel('Hand history')).toBeVisible()
  await expect(page.getByLabel('Session activity')).toContainText('Seed 7')
  await expect(page.getByLabel('Hand status')).toContainText(/Hand\s*1/)

  for (let step = 0; step < 16; step += 1) {
    if (!(await clickPreferredAction(page))) {
      break
    }
  }

  await expect(page.getByLabel('Hand status')).toContainText('terminal')
  await expect(
    page.getByLabel('Available actions').getByText('This hand is complete.'),
  ).toBeVisible()

  await page.getByRole('button', { name: 'Deal next hand' }).click()

  await expect(page.getByLabel('Hand status')).toContainText(/Hand\s*2/)
  await expect(page.getByLabel('Session activity')).toContainText('Ready')
})

test('shows a recoverable error when initialization fails', async ({ page }) => {
  await page.goto('/')

  await page.evaluate(() => {
    ;(window as typeof window & { __GTO_FORCE_WORKER_ERROR__?: string }).__GTO_FORCE_WORKER_ERROR__ =
      'forced initialization failure for e2e'
  })

  await page.getByRole('button', { name: 'Restart session' }).click()

  await expect(page.getByRole('alert')).toContainText(
    'forced initialization failure for e2e',
  )
  await page.getByRole('button', { name: 'Retry session' }).click()

  await expect(page.getByRole('alert')).toHaveCount(0)
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await expect(page.getByLabel('Available actions')).toContainText(/Call|Check/)
})

test('switches into Hybrid Play mode for production-style browser sessions', async ({
  page,
}) => {
  await page.goto('/')

  await page.getByRole('button', { name: /Hybrid Play/i }).click()
  await page.getByRole('button', { name: 'Restart session' }).click()

  await expect(page.getByLabel('Session activity')).toContainText('Hybrid Play')
  await expect(page.getByLabel('Hand status')).toContainText('Hybrid Play')
})

async function clickPreferredAction(page: Page): Promise<boolean> {
  const activity = page.getByLabel('Session activity')
  const actions = page.getByLabel('Available actions')
  await expect(activity).toContainText('Ready')
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
    throw new Error('No enabled action buttons were available')
  }

  await actions.getByRole('button', { name: label, exact: true }).click()
  return true
}
