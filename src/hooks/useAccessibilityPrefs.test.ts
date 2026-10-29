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
  type Native = typeof native
  let mode: 'immediate' | 'deferred' | 'reject' = 'immediate'
  const deferred: Array<PromiseWithResolvers<Native>> = []
  mockIPC((command) => {
    assert.equal(command, 'native_accessibility')
    reads += 1
    if (mode === 'reject') {
      throw new Error('native unavailable')
    }
    if (mode === 'deferred') {
      const gate = Promise.withResolvers<Native>()
      deferred.push(gate)

      return gate.promise
    }

    return native
  })

  const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
  const { userEvent } = await import('@testing-library/user-event')
  const { NATIVE_ACCESSIBILITY_EVENT } = await import('../services/mobile')
  const { useAccessibilityPrefs } = await import('./useAccessibilityPrefs')

  beforeEach(() => {
    cleanup()
    reads = 0
    mode = 'immediate'
    deferred.length = 0
    document.documentElement.removeAttribute('data-keyboard-focus')
    native = { reduceTransparency: true, increaseContrast: true }
  })
  after(async () => {
    cleanup()
    await GlobalRegistrator.unregister()
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

  const flag = (name: string) => document.documentElement.hasAttribute(name)

  test('a rejected native read leaves the web preferences in effect and a later read recovers', async () => {
    mode = 'reject'
    renderHook(() => useAccessibilityPrefs())
    await waitFor(() => assert.equal(reads, 1))
    assert.equal(flag('data-reduce-transparency'), false)

    mode = 'immediate'
    window.dispatchEvent(new Event(NATIVE_ACCESSIBILITY_EVENT))
    await waitFor(() => assert.equal(flag('data-reduce-transparency'), true))
  })

  test('an older native response cannot overwrite a newer one', async () => {
    mode = 'deferred'
    renderHook(() => useAccessibilityPrefs())
    window.dispatchEvent(new Event(NATIVE_ACCESSIBILITY_EVENT))
    await waitFor(() => assert.equal(deferred.length, 2))

    await act(async () => {
      deferred[1]?.resolve({ reduceTransparency: false, increaseContrast: false })
      await deferred[1]?.promise
    })
    await act(async () => {
      deferred[0]?.resolve({ reduceTransparency: true, increaseContrast: true })
      await deferred[0]?.promise
    })

    assert.equal(flag('data-reduce-transparency'), false)
    assert.equal(flag('data-increase-contrast'), false)
  })

  test('a native response that arrives after unmount changes nothing', async () => {
    mode = 'deferred'
    const { unmount } = renderHook(() => useAccessibilityPrefs())
    await waitFor(() => assert.equal(deferred.length, 1))
    unmount()

    await act(async () => {
      deferred[0]?.resolve({ reduceTransparency: true, increaseContrast: true })
      await deferred[0]?.promise
    })

    assert.equal(flag('data-reduce-transparency'), false)
  })

  test('keyboard navigation shows focus rings and pointer input hides them', async () => {
    const user = userEvent.setup({ document })
    renderHook(() => useAccessibilityPrefs())
    await waitFor(() => assert.equal(reads, 1))

    await user.keyboard('{Tab}')
    assert.equal(flag('data-keyboard-focus'), true)

    await user.pointer({ target: document.body, keys: '[MouseLeft]' })
    assert.equal(flag('data-keyboard-focus'), false)

    await user.keyboard('a')
    assert.equal(flag('data-keyboard-focus'), false)
  })

  test('media-query changes update the attributes', async () => {
    const original = window.matchMedia
    const listeners = new Set<() => void>()
    const state = { matches: false }
    class FakeQuery extends EventTarget implements MediaQueryList {
      onchange = null
      constructor(readonly media: string) {
        super()
      }
      get matches() {
        return this.media.includes('transparency') && state.matches
      }
      addEventListener(_type: string, listener: EventListenerOrEventListenerObject | null) {
        if (typeof listener === 'function') {
          listeners.add(() => listener(new Event('change')))
        }
      }
      addListener() {}
      removeListener() {}
    }

    window.matchMedia = (query: string) => new FakeQuery(query)
    native = { reduceTransparency: false, increaseContrast: false }

    try {
      renderHook(() => useAccessibilityPrefs())
      await waitFor(() => assert.equal(reads, 1))
      assert.equal(flag('data-reduce-transparency'), false)

      state.matches = true
      act(() => {
        for (const listener of listeners) {
          listener()
        }
      })
      assert.equal(flag('data-reduce-transparency'), true)
    } finally {
      window.matchMedia = original
    }
  })
}
