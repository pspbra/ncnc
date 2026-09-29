import { test, expect } from '@playwright/test'

test('shows the login page', async ({ page }) => {
  await page.goto('/webui/login')
  await expect(page.getByPlaceholder('密码')).toBeVisible()
})
