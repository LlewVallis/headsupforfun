import { expect, test } from '@playwright/test'

test('renders the poker bootstrap screen', async ({ page }) => {
  await page.goto('/')

  await expect(
    page.getByRole('heading', { name: 'GTO Poker' }),
  ).toBeVisible()
  await expect(page.getByLabel('Poker table preview')).toBeVisible()
  await expect(page.getByText('Next implementation step')).toBeVisible()
})
