import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

if (process.env.PIPEDASH_SHORTCUTS_NATIVE_TEST !== '1') {
  test('native menu shortcuts run in an isolated desktop runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_SHORTCUTS_NATIVE_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 1400, height: 900 })

  const callbacks = new Map<number, (event: unknown) => void>()
  const invocations: string[] = []
  let nextCallback = 0
  let registration: PromiseWithResolvers<number> | null = null
  let rejectRegistration = false

  Object.assign(window, {
    __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
    __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
    __TAURI_INTERNALS__: {
      transformCallback: (callback: (event: unknown) => void) => {
        nextCallback += 1
        callbacks.set(nextCallback, callback)

        return nextCallback
      },
      invoke: async (command: string, args: Record<string, unknown> = {}) => {
        invocations.push(command)
        if (command === 'plugin:event|listen') {
          if (rejectRegistration) {
            throw new Error('listen refused')
          }

          return registration ? registration.promise : args.handler
        }

        return undefined
      },
    },
  })

  const { act, cleanup, renderHook } = await import('@testing-library/react')
  const { useAppShortcuts } = await import('./useAppShortcuts')

  beforeEach(() => {
    cleanup()
    callbacks.clear()
    invocations.length = 0
    registration = null
    rejectRegistration = false
  })

  after(() => GlobalRegistrator.unregister())

  const count = (command: string) => invocations.filter((name) => name === command).length

  async function emit(payload: string) {
    await act(async () => {
      for (const callback of callbacks.values()) {
        callback({ event: 'menu-command', id: 1, payload })
      }
    })
  }

  test('a rejected menu registration is logged and the hook keeps working', async (context) => {
    const error = context.mock.method(console, 'error', () => undefined)
    rejectRegistration = true

    const { unmount } = renderHook(() => useAppShortcuts({ onRefresh: () => undefined }))
    await act(async () => undefined)

    assert.equal(error.mock.calls.length, 1)
    assert.equal(count('plugin:event|unlisten'), 0)
    unmount()
    mock.restoreAll()
  })

  test('unmounting before registration settles unregisters once and ignores later commands', async () => {
    registration = Promise.withResolvers<number>()
    let toggled = 0

    const { unmount } = renderHook(() =>
      useAppShortcuts({
        onRefresh: () => undefined,
        onSidebar: () => {
          toggled += 1
        },
      })
    )
    await act(async () => undefined)
    unmount()
    await emit('toggle-sidebar')
    assert.equal(toggled, 0)
    assert.equal(count('plugin:event|unlisten'), 0)

    await act(async () => {
      registration?.resolve(1)
      await registration?.promise
    })
    await act(async () => undefined)

    assert.equal(count('plugin:event|unlisten'), 1)
    assert.equal(toggled, 0)
  })

  test('a replaced callback runs instead of the old one without registering again', async () => {
    const calls: string[] = []
    const { rerender } = renderHook(
      ({ label }) => useAppShortcuts({ onRefresh: () => undefined, onSidebar: () => calls.push(label) }),
      {
        initialProps: { label: 'old' },
      }
    )
    await act(async () => undefined)

    rerender({ label: 'new' })
    await emit('toggle-sidebar')

    assert.deepEqual(calls, ['new'])
    assert.equal(count('plugin:event|listen'), 1)
  })
}
