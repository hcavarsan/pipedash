import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { mkdir } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'
import { failure, fixedNow, mockResponse, notifications, pipelines, release } from './capture-fixtures.mjs'

const baseUrl = process.env.CAPTURE_APP_URL || 'http://127.0.0.1:5198'
const origin = new URL(baseUrl)
assert(['localhost', '127.0.0.1', '[::1]'].includes(origin.hostname), 'Capture must use a loopback app server')
const output = fileURLToPath(new URL('../public/screenshots/apple/', import.meta.url))
const executablePath = process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined)
const browser = await chromium.launch({ executablePath, headless: true, args: ['--no-sandbox'] })
const devices = {
  iphone: { width: 440, height: 956, scale: 3, agent: 'iPhone; CPU iPhone OS 18_0 like Mac OS X' },
  ipad: { width: 1032, height: 1376, scale: 2, agent: 'iPad; CPU OS 18_0 like Mac OS X' },
}

try {
  for (const [device, settings] of Object.entries(devices)) {
    const failures = []
    const context = await browser.newContext({
      viewport: { width: settings.width, height: settings.height }, deviceScaleFactor: settings.scale,
      isMobile: true, hasTouch: true, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'dark', reducedMotion: 'reduce',
      userAgent: `Mozilla/5.0 (${settings.agent}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1`,
    })
    await context.route('**/*', async (route) => {
      const request = route.request()
      const url = new URL(request.url())
      try {
        assert.equal(url.origin, origin.origin, `External request blocked: ${url.origin}`)
        if (url.pathname.startsWith('/api/v1/')) {
          await route.fulfill({ json: mockResponse(request.method(), url) })
        } else {
          await route.continue()
        }
      } catch (error) {
        failures.push(error.message)
        await route.fulfill({ status: 418, body: error.message })
      }
    })
    await context.routeWebSocket('**/api/v1/ws', () => {})
    await context.addInitScript(({ now, pins }) => {
      const NativeDate = Date
      globalThis.Date = class extends NativeDate {
        constructor(...args) { super(...(args.length ? args : [now])) }
        static now() { return now }
      }
      localStorage.setItem('pipedash-pinned-pipelines', JSON.stringify({ state: { pinnedKeys: pins }, version: 0 }))
      localStorage.setItem('mantine-color-scheme-value', 'dark')
    }, {
      now: fixedNow,
      pins: [`GitHub:${release.id.replace('__1__', '__')}`, `GitLab:${pipelines[2].id.replace('__2__', '__')}`],
    })
    const page = await context.newPage()
    page.setDefaultTimeout(20_000)
    page.on('pageerror', (error) => failures.push(error.message))
    const folder = `${output}/${device}/en`
    await mkdir(folder, { recursive: true })
    const prepare = async (path = '/pipelines') => {
      await page.goto(`${baseUrl}${path}`)
      await page.getByText(path === '/pipelines' ? 'Release iOS app' : 'API integration tests', { exact: true }).first().waitFor()
      await page.evaluate(async ({ items, phone }) => {
        const { useNotificationCenterStore } = await import('/src/stores/notificationCenterStore.ts')
        useNotificationCenterStore.setState({ items })
        if (phone) {
          document.documentElement.style.setProperty('--native-safe-area-top', '62px')
          document.documentElement.style.setProperty('--native-safe-area-bottom', '34px')
        }
      }, { items: notifications, phone: device === 'iphone' })
      await page.addStyleTag({ content: '.tsqd-parent-container { display: none !important; }' })
    }
    const capture = async (number, expected) => {
      await page.getByText(expected, { exact: false }).first().waitFor()
      await page.evaluate(async () => {
        await document.fonts.ready
        await Promise.all([...document.images].map((image) => image.decode().catch(() => {})))
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      })
      assert.deepEqual(failures, [], `Capture ${device}/${number} made unexpected requests or raised errors`)
      const path = `${folder}/${number}.png`
      const bytes = await page.screenshot({ path, animations: 'disabled' })
      assert.equal(bytes.readUInt32BE(16), settings.width * settings.scale)
      assert.equal(bytes.readUInt32BE(20), settings.height * settings.scale)
      console.log(`${device}/${number}.png ${settings.width * settings.scale}x${settings.height * settings.scale}`)
    }

    await prepare()
    await capture('01', 'Argo CD')
    if (device === 'iphone') {
      await page.getByRole('button', { name: 'Actions for Release iOS app' }).click()
      await page.getByRole('menuitem', { name: 'Trigger', exact: true }).click()
    } else {
      await page.getByRole('button', { name: 'Trigger Release iOS app', exact: true }).first().click()
    }
    await page.getByLabel('Release version', { exact: false }).waitFor()
    assert.equal(await page.getByLabel('Release version', { exact: false }).inputValue(), '2.8.0')
    await capture('02', 'Run tests before release')

    await prepare(`/pipelines/${encodeURIComponent(failure.id)}`)
    await capture('03', 'Validate checkout API contract')

    await prepare()
    await page.evaluate(async () => {
      const { useNotificationCenterStore } = await import('/src/stores/notificationCenterStore.ts')
      useNotificationCenterStore.getState().open({ type: 'provider', providerId: 1 })
    })
    await capture('04', 'Actions jobs are delayed')

    await prepare()
    if (device === 'iphone') {
      await page.getByRole('link', { name: /^Notifications/ }).click()
    } else {
      await page.getByRole('button', { name: /Notifications/ }).click()
    }
    await capture('05', 'API integration tests failed')
    await context.close()
  }
} finally {
  await browser.close()
}
