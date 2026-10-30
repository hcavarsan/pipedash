import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdir } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'
import { failure, fixedNow, mockResponse, notifications, pipelines, release } from './capture-fixtures.mjs'

// Screenshots are generated, never committed: run this before export:fastlane (the prepare_screenshots lane does).
const baseUrl = process.env.CAPTURE_APP_URL || 'http://127.0.0.1:5198'
const origin = new URL(baseUrl)
assert(['localhost', '127.0.0.1', '[::1]'].includes(origin.hostname), 'Capture must use a loopback app server')
const output = fileURLToPath(new URL('../public/screenshots/', import.meta.url))
const executablePath = process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined)

// Without CAPTURE_APP_URL, serve the app from the repository root with Vite's dev server; the capture
// imports app stores through it, so a production build does not work here.
async function startVite() {
  const root = fileURLToPath(new URL('../../../', import.meta.url))
  const env = { ...process.env, BROWSER: 'none' }
  delete env.TAURI_ARCH
  delete env.TAURI_DEV_HOST
  const child = spawn(`${root}node_modules/.bin/vite`, ['--port', origin.port, '--strictPort', '--host', origin.hostname], { cwd: root, env, stdio: ['ignore', 'ignore', 'inherit'] })
  const exited = new Promise((_, reject) => child.once('exit', (code) => reject(new Error(`Vite exited with code ${code}`))))
  try {
    for (const deadline = Date.now() + 120_000; ;) {
      assert(Date.now() < deadline, 'Vite did not start within 120s')
      if (await Promise.race([fetch(baseUrl).then((response) => response.ok, () => false), exited])) return child
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
  } catch (error) {
    child.kill()
    throw error
  }
}
const vite = process.env.CAPTURE_APP_URL ? null : await startVite()
let browser
// iPhone and iPad feed the App Store decks. Desktop and iPhone, as webp, feed the README through
// pipedash-website/public/img/readme/{desktop,mobile}/NN-<page>.webp.
const devices = {
  iphone: { width: 440, height: 956, scale: 3, mobile: true, phone: true, safeArea: { top: '62px', bottom: '34px' }, folder: 'apple/iphone', agent: 'iPhone; CPU iPhone OS 18_0 like Mac OS X' },
  ipad: { width: 1032, height: 1376, scale: 2, mobile: true, folder: 'apple/ipad', agent: 'iPad; CPU OS 18_0 like Mac OS X' },
  'android-phone': { width: 360, height: 780, scale: 3, mobile: true, phone: true, safeArea: { top: '32px', bottom: '24px' }, folder: 'android/phone', agent: 'Linux; Android 15; Pixel 9' },
  'android-tablet': { width: 800, height: 1280, scale: 2, mobile: true, folder: 'android/tablet', agent: 'iPad; CPU OS 18_0 like Mac OS X' },
  desktop: { width: 1280, height: 800, scale: 2, mobile: false, folder: 'desktop', agent: 'Macintosh; Intel Mac OS X 10_15_7' },
}

try {
  browser = await chromium.launch({ executablePath, headless: true, args: ['--no-sandbox'] })
  for (const [device, settings] of Object.entries(devices)) {
    const failures = []
    const context = await browser.newContext({
      viewport: { width: settings.width, height: settings.height }, deviceScaleFactor: settings.scale,
      isMobile: settings.mobile, hasTouch: settings.mobile, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'dark', reducedMotion: 'reduce',
      userAgent: settings.mobile
        ? `Mozilla/5.0 (${settings.agent}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1`
        : `Mozilla/5.0 (${settings.agent}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15`,
    })
    await context.route('**/*', async (route) => {
      const request = route.request()
      const url = new URL(request.url())
      try {
        assert.equal(url.origin, origin.origin, `External request blocked: ${url.origin}`)
        if (url.pathname.startsWith('/api/v1/')) {
          await route.fulfill({ json: mockResponse(request.method(), url, request.postDataJSON()) })
        } else {
          await route.continue()
        }
      } catch (error) {
        failures.push(error.message)
        await route.fulfill({ status: 418, body: error.message })
      }
    })
    await context.routeWebSocket('**/api/v1/ws', () => {})
    await context.addInitScript(({ now }) => {
      const NativeDate = Date
      globalThis.Date = class extends NativeDate {
        constructor(...args) { super(...(args.length ? args : [now])) }
        static now() { return now }
      }
      localStorage.setItem('mantine-color-scheme-value', 'dark')
    }, { now: fixedNow })
    const page = await context.newPage()
    page.setDefaultTimeout(20_000)
    page.on('pageerror', (error) => failures.push(error.message))
    const folder = `${output}/${settings.folder}/en`
    await mkdir(folder, { recursive: true })
    const phone = settings.phone === true
    const list = phone ? `/pipelines/groups/${encodeURIComponent(`repo:${release.repository}`)}` : '/pipelines'
    const prepare = async (path = list, ready = 'Release iOS app') => {
      await page.goto(`${baseUrl}${path}`)
      await page.getByText(ready, { exact: true }).first().waitFor()
      await page.evaluate(async ({ items, safeArea }) => {
        const { useNotificationCenterStore } = await import('/src/stores/notificationCenterStore.ts')
        useNotificationCenterStore.setState({ items })
        if (safeArea) {
          document.documentElement.style.setProperty('--native-safe-area-top', safeArea.top)
          document.documentElement.style.setProperty('--native-safe-area-bottom', safeArea.bottom)
        }
      }, { items: notifications, safeArea: settings.safeArea })
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
    // One page per number, in the order a user meets them: spot a failure, dig into it, act on it, then the wider picture.
    await (phone ? prepare('/pipelines', 'platform-api') : prepare())
    await capture('01', phone ? 'storefront' : 'Argo CD')

    const history = `/pipelines/${encodeURIComponent(failure.id)}`
    await prepare(history, 'API integration tests')
    await capture('02', 'Validate checkout API contract')

    await page.getByText('Validate checkout API contract', { exact: false }).first().click()
    if (!phone) await page.getByRole('tab', { name: 'Details', exact: true }).click()
    await capture('03', phone ? 'Contract tests' : 'Actor')

    if (phone) {
      await page.getByText('Contract tests', { exact: true }).click()
    } else {
      await page.getByRole('tab', { name: 'Logs', exact: true }).click()
    }
    await capture('04', 'Expected status 201, received 422')

    await prepare()
    if (phone) {
      // The row covers its menu button on phones, where a swipe reveals it, so the click is dispatched.
      await page.getByRole('button', { name: 'Actions for Release iOS app' }).first().dispatchEvent('click')
      await page.getByRole('menuitem', { name: 'Trigger', exact: true }).click()
    } else {
      await page.getByRole('button', { name: 'Trigger Release iOS app', exact: true }).first().click()
    }
    await page.getByLabel('Release version', { exact: false }).waitFor()
    assert.equal(await page.getByLabel('Release version', { exact: false }).inputValue(), '2.8.0')
    await capture('05', 'Run tests before release')

    await prepare(`${history}?tab=metrics&dateRange=7d&period=daily`, 'API integration tests')
    await page.getByText('Loading chart', { exact: false }).first().waitFor({ state: 'detached' })
    await capture('06', 'Total runs per day')

    await prepare()
    await page.evaluate(async () => {
      const { useNotificationCenterStore } = await import('/src/stores/notificationCenterStore.ts')
      useNotificationCenterStore.getState().open({ type: 'provider', providerId: 1 })
    })
    await capture('07', 'Actions jobs are delayed')

    await prepare()
    if (phone) {
      await page.getByRole('link', { name: /^Notifications/ }).click()
    } else {
      await page.getByRole('button', { name: /Notifications/ }).click()
    }
    await capture('08', 'API integration tests failed')

    await prepare(phone ? '/browse' : '/settings/providers', 'Argo CD')
    if (!phone) await page.getByRole('button', { name: 'Edit refresh interval for GitHub', exact: true }).waitFor()
    await capture('09', 'Jenkins')
    await context.close()
  }
} finally {
  await browser?.close()
  vite?.kill()
}
