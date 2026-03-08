import { expect, test } from '@playwright/test'

test('renders the poker bootstrap screen', async ({ page }) => {
  await page.goto('/')

  await expect(
    page.getByRole('heading', { name: 'GTO Poker' }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Call' })).toBeVisible()
  await expect(page.getByLabel('Hand history')).toBeVisible()
})
