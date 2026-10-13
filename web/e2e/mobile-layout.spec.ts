import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Layout guards for the two mobile blockers that made the interrogation page
// unplayable on a phone: the fixed workspace tab bar covering the send button,
// and the prologue overlay clipping its own call to action.
//
// These checks run the real stylesheet against DOM skeletons that mirror
// InvestigationPage.vue, so they need no backend and no live case data.

const webRoot = dirname(fileURLToPath(import.meta.url))
const stylesheet = readFileSync(resolve(webRoot, '../src/style.css'), 'utf8')

function document(markup: string): string {
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>${stylesheet}</style>
  </head>
  <body style="margin: 0">${markup}</body>
</html>`
}

/** A session that is already in progress: composer visible, no prologue. */
function interrogationMarkup(): string {
  return document(`<div class="investigation-shell">
      <header class="app-header"><span class="wordmark">谜局</span></header>
      <main class="investigation-grid" data-mobile-tab="dialogue">
        <section class="dialogue-panel workspace-dialogue">
          <header class="dialogue-heading"><span>罗成</span></header>
          <div class="transcript"><article class="transcript-turn">对话</article></div>
          <div class="composer-region" data-evidence-dropzone>
            <div class="attachment-summary"><span>已附加 0 条线索</span></div>
            <div class="composer-quick-actions">
              <button class="quick-action-btn" type="button">追问详情</button>
              <button class="quick-action-btn" type="button">指出矛盾</button>
              <button class="quick-action-btn" type="button">继续陈述</button>
            </div>
            <div class="composer-box">
              <textarea maxlength="2000"></textarea>
              <footer>
                <span>0/2000</span>
                <button class="primary-button send-button" type="button" data-testid="send">发送</button>
              </footer>
            </div>
          </div>
        </section>
        <aside class="people-panel workspace-people"><section>相关人物</section></aside>
        <aside class="research-panel workspace-research"><footer class="research-footer">revision 0</footer></aside>
      </main>
      <nav class="mobile-workspace-tabs">
        <button type="button">人物</button>
        <button type="button" class="selected">对话</button>
        <button type="button">调查记录</button>
      </nav>
    </div>`)
}

/** The modal that a brand-new session opens with. */
function prologueMarkup(cardPadding = ''): string {
  return document(`<div class="investigation-shell">
      <header class="app-header"><span class="wordmark">谜局</span></header>
      <main class="investigation-grid" data-mobile-tab="dialogue">
        <section class="dialogue-panel workspace-dialogue"><div class="composer-region"></div></section>
      </main>
      <nav class="mobile-workspace-tabs"><button type="button">对话</button></nav>
    </div>
    <section class="investigation-prologue" role="dialog" aria-modal="true" data-testid="prologue">
      <div class="prologue-card" style="${cardPadding}">
        <span>调查开始 · CASE OPEN</span>
        <h1>雨夜画廊失窃案</h1>
        <p class="prologue-summary">
          一件密封待拍画作在闭馆后的二十分钟内从私人画廊消失。三名当晚在场人员各怀秘密，其中一人策划了盗窃。
        </p>
        <ol>
          <li><strong>先听说法</strong><span>选择一名人物，从事情经过开始询问，建立完整说法。</span></li>
          <li><strong>出示线索</strong><span>把已发现线索附加到问题中，推动审讯阶段并解锁新线索。</span></li>
          <li><strong>追问矛盾</strong><span>用新解锁的线索继续追问不同嫌疑人，找出完整证据链。</span></li>
        </ol>
        <small>本局真相已经锁定；你的行动会改变人物如何回应，不会改写事实。</small>
        <button class="primary-button" type="button" data-testid="enter">进入调查</button>
      </div>
    </section>`)
}

async function hitTest(
  page: import('@playwright/test').Page,
  box: { x: number; y: number; width: number; height: number },
): Promise<string | null> {
  return page.evaluate(
    ([x, y]) => {
      const element = document.elementFromPoint(x as number, y as number)
      const button = element?.closest('button')
      return button?.textContent?.trim() ?? element?.tagName ?? null
    },
    [box.x + box.width / 2, box.y + box.height / 2] as const,
  )
}

test.describe('mobile investigation layout', () => {
  test.use({ viewport: { width: 390, height: 844 } })

  test('fixed workspace tab bar never covers the composer send button', async ({ page }) => {
    await page.setContent(interrogationMarkup())
    await page.waitForFunction(() => document.readyState === 'complete')

    const send = page.getByTestId('send')
    const tabBar = page.locator('.mobile-workspace-tabs')
    const sendBox = (await send.boundingBox())!
    const tabBox = (await tabBar.boundingBox())!

    expect(
      sendBox.y + sendBox.height,
      'the send button bottom edge must stay above the fixed tab bar',
    ).toBeLessThanOrEqual(tabBox.y)
    expect(tabBox.y + tabBox.height).toBeLessThanOrEqual(844)
    expect(await hitTest(page, sendBox)).toBe('发送')
  })

  test('every workspace tab stays reachable', async ({ page }) => {
    await page.setContent(interrogationMarkup())
    await page.waitForFunction(() => document.readyState === 'complete')

    const buttons = await page.locator('.mobile-workspace-tabs button').all()
    expect(buttons).toHaveLength(3)
    for (const button of buttons) {
      const box = (await button.boundingBox())!
      expect(await hitTest(page, box)).toBe((await button.textContent())?.trim())
    }
  })

  test('prologue call to action is reachable when it fits', async ({ page }) => {
    await page.setContent(prologueMarkup())
    await page.waitForFunction(() => document.readyState === 'complete')

    const prologue = page.getByTestId('prologue')
    const enter = page.getByTestId('enter')
    const enterBox = (await enter.boundingBox())!
    const shellBox = (await prologue.boundingBox())!

    expect(enterBox.y).toBeGreaterThanOrEqual(shellBox.y)
    expect(enterBox.y + enterBox.height).toBeLessThanOrEqual(shellBox.y + shellBox.height)
    expect(await hitTest(page, enterBox)).toBe('进入调查')
    expect(await prologue.evaluate((element) => getComputedStyle(element).overflowY)).toBe('auto')
  })

  test('prologue call to action survives content taller than the phone', async ({ page }) => {
    // A long case title/summary pushes the card past the viewport, which used to
    // be clipped outright and made "进入调查" unreachable.
    await page.setContent(prologueMarkup('padding-bottom: 900px;'))
    await page.waitForFunction(() => document.readyState === 'complete')

    const prologue = page.getByTestId('prologue')
    await page.getByTestId('enter').scrollIntoViewIfNeeded()
    const enterBox = (await page.getByTestId('enter').boundingBox())!

    expect(
      await prologue.evaluate((element) => element.scrollHeight > element.clientHeight),
      'the overlay must be able to scroll its own content',
    ).toBe(true)
    expect(
      await prologue.evaluate((element) => getComputedStyle(element).overflowY),
      'a clipped overlay hides the call to action; it must be user-scrollable',
    ).toBe('auto')
    expect(enterBox.y + enterBox.height).toBeLessThanOrEqual(844)
    expect(await hitTest(page, enterBox)).toBe('进入调查')
  })

  test('prologue stays centred and unscrolled on a wide screen', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 })
    await page.setContent(prologueMarkup())
    await page.waitForFunction(() => document.readyState === 'complete')

    const prologue = page.getByTestId('prologue')
    const cardBox = (await page.locator('.prologue-card').boundingBox())!
    const shellBox = (await prologue.boundingBox())!

    expect(cardBox.y).toBeGreaterThanOrEqual(shellBox.y)
    expect(cardBox.y + cardBox.height).toBeLessThanOrEqual(shellBox.y + shellBox.height)
    expect(await prologue.evaluate((element) => element.scrollTop)).toBe(0)
  })

  test('accusation dialog keeps its footer inside the viewport', async ({ page }) => {
    await page.setContent(
      document(`<div class="dialog-layer">
          <section class="accusation-dialog" data-testid="dialog">
            <header>提交判断</header>
            <div class="accusation-body" style="min-height: 1200px">线索</div>
            <footer><button type="button" data-testid="confirm">确认判断</button></footer>
          </section>
        </div>`),
    )
    await page.waitForFunction(() => document.readyState === 'complete')

    const dialog = page.getByTestId('dialog')
    const dialogBox = (await dialog.boundingBox())!
    expect(dialogBox.y + dialogBox.height).toBeLessThanOrEqual(844)

    await page.getByTestId('confirm').scrollIntoViewIfNeeded()
    const confirmBox = (await page.getByTestId('confirm').boundingBox())!
    expect(confirmBox.y + confirmBox.height).toBeLessThanOrEqual(844)
    expect(await hitTest(page, confirmBox)).toBe('确认判断')
  })
})
