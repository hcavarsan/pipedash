import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

const mode = process.env.PIPEDASH_PLATFORM_TEST

if (mode === undefined) {
  for (const name of ['browser', 'ios']) {
    test(`platform detection in an isolated ${name} runtime`, () => {
      const result = spawnSync(process.execPath, ['test', import.meta.filename], {
        env: { ...process.env, PIPEDASH_PLATFORM_TEST: name },
        encoding: 'utf8',
        timeout: 30000,
      })
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
    })
  }
} else {
  GlobalRegistrator.register({
    url: 'http://localhost/',
    settings: { navigator: { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)' } },
  })
  if (mode === 'ios') {
    Object.assign(window, {
      __TAURI_INTERNALS__: {},
      __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' },
    })
  }
  after(() => GlobalRegistrator.unregister())

  const { getPlatformOverride, isDesktopTauri, isMobilePlatform, platform, setPlatformOverride } = await import(
    './platform'
  )

  if (mode === 'browser') {
    test('an unset browser falls back to user agent detection', async () => {
      assert.equal(getPlatformOverride(), null)
      assert.equal(await platform(), 'macos')
    })

    test('a platform override wins over detection and clearing it restores detection', async () => {
      setPlatformOverride('windows')
      assert.equal(await platform(), 'windows')

      setPlatformOverride('linux')
      assert.equal(await platform(), 'linux')

      setPlatformOverride(null)
      assert.equal(await platform(), 'macos')
    })

    test('unknown stored overrides are ignored', async () => {
      localStorage.setItem('platform-override', 'beos')
      assert.equal(getPlatformOverride(), null)
      assert.equal(await platform(), 'macos')
      localStorage.removeItem('platform-override')
    })
  } else {
    test('a native mobile runtime ignores the stored override', async () => {
      localStorage.setItem('platform-override', 'windows')

      assert.equal(isMobilePlatform(), true)
      assert.equal(isDesktopTauri(), false)
      assert.equal(await platform(), 'ios')
    })
  }
}
