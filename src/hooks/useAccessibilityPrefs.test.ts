import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'

if (process.env.PIPEDASH_ACCESSIBILITY_TEST !== '1') {
  test('native accessibility follows iPad settings and lifecycle in an isolated runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_ACCESSIBILITY_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 1024, height: 768 })
  Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })
  let native = { reduceTransparency: true, increaseContrast: true }
  let reads = 0
  mockIPC((command) => {
    assert.equal(command, 'native_accessibility')
    reads += 1
    return native
  })

  const { cleanup, renderHook, waitFor } = await import('@testing-library/react')
  const { NATIVE_ACCESSIBILITY_EVENT } = await import('../services/mobile')
  const { useAccessibilityPrefs } = await import('./useAccessibilityPrefs')

  beforeEach(() => {
    cleanup()
    reads = 0
    native = { reduceTransparency: true, increaseContrast: true }
  })
  after(() => {
    cleanup()
    GlobalRegistrator.unregister()
  })

  test('native accessibility settings apply on an iPad wider than the mobile breakpoint', async () => {
    assert.equal(window.matchMedia('(max-width: 768px)').matches, false)
    renderHook(() => useAccessibilityPrefs())

    await waitFor(() => {
      assert.equal(document.documentElement.hasAttribute('data-reduce-transparency'), true)
      assert.equal(document.documentElement.hasAttribute('data-increase-contrast'), true)
    })
    assert.equal(reads, 1)
  })

  test('native accessibility changes refresh without a web visibility or media-query event', async () => {
    renderHook(() => useAccessibilityPrefs())
    await waitFor(() => assert.equal(document.documentElement.hasAttribute('data-increase-contrast'), true))

    native = { reduceTransparency: false, increaseContrast: false }
    window.dispatchEvent(new Event(NATIVE_ACCESSIBILITY_EVENT))

    await waitFor(() => {
      assert.equal(document.documentElement.hasAttribute('data-reduce-transparency'), false)
      assert.equal(document.documentElement.hasAttribute('data-increase-contrast'), false)
    })
    assert.equal(reads, 2)
  })

  test('unmount removes the native listener and accessibility attributes', async () => {
    const { unmount } = renderHook(() => useAccessibilityPrefs())
    await waitFor(() => assert.equal(document.documentElement.hasAttribute('data-increase-contrast'), true))

    unmount()
    window.dispatchEvent(new Event(NATIVE_ACCESSIBILITY_EVENT))

    assert.equal(reads, 1)
    assert.equal(document.documentElement.hasAttribute('data-reduce-transparency'), false)
    assert.equal(document.documentElement.hasAttribute('data-increase-contrast'), false)
  })
}
