import { expect, test } from '@playwright/test'

const screenshotDirectory = process.env.QA_SCREENSHOT_DIR

test('app loads and completes the primary mock interaction', async ({ page }) => {
  const consoleProblems: string[] = []
  page.on('console', (message) => {
    if (message.type() === 'error' || message.type() === 'warning') {
      consoleProblems.push(`${message.type()}: ${message.text()}`)
    }
  })
  page.on('pageerror', (error) => consoleProblems.push(`pageerror: ${error.message}`))

  await page.goto('/')
  await expect(page).toHaveTitle(/NarraState/)
  const heroHeading = page.getByRole('heading', { level: 1 })
  await expect(heroHeading).toContainText('真相不会改变。')
  await expect(heroHeading).toContainText('人会。')

  const enterCase = page.getByRole('link', { name: /进入谜局/ })
  await expect(enterCase).toHaveAttribute('href', /\/cases\/rain-gallery$/)
  await enterCase.click()
  await expect(page.getByRole('heading', { name: '雨夜画廊失窃案' })).toBeVisible()
  await page.getByRole('button', { name: /开始调查/ }).click()

  await expect(page).toHaveURL(/\/sessions\/[0-9a-f-]+$/)
  await expect(page.getByText('从一个具体问题开始')).toBeVisible()
  const composer = page.getByPlaceholder('输入你的问题')
  await composer.fill('闭馆后你在哪里？')
  await page.getByRole('button', { name: /发送/ }).click()

  await expect(page.getByText('闭馆后你在哪里？')).toBeVisible()
  await expect(page.locator('.transcript-turn:not(.player)').filter({ hasText: '我会回答你的问题。' })).toBeVisible()
  await expect(page.locator('.research-footer')).toContainText('revision 1')
  expect(consoleProblems).toEqual([])
  if (screenshotDirectory) {
    await page.screenshot({ path: `${screenshotDirectory}/investigation-desktop.png`, fullPage: false })
  }
})

test('mobile layout exposes workspace navigation without clipping the shell', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/')
  const heroHeading = page.getByRole('heading', { level: 1 })
  await expect(heroHeading).toContainText('真相不会改变。')
  await expect(heroHeading).toContainText('人会。')
  await expect(page.locator('body')).not.toHaveCSS('overflow-x', 'scroll')
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true)
  if (screenshotDirectory) {
    await page.screenshot({ path: `${screenshotDirectory}/home-mobile.png`, fullPage: false })
  }
})
