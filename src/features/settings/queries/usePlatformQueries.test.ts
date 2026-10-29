import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

if (process.env.PIPEDASH_RESET_TEST !== '1') {
  test('factory reset reports partial failures without discarding preferences in an isolated runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_RESET_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 390, height: 844 })
  // Static imports initialize browser and platform state before this test can install its isolated DOM and IPC bridge.
  const { notifications } = await import('@mantine/notifications')
  const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { mockIPC } = await import('@tauri-apps/api/mocks')
  const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')

  const complete = { providers_removed: 1, caches_cleared: true, tokens_cleared: true, metrics_cleared: true }
  let response = { ...complete }
  let failure: Error | null = null
  let gate: Promise<void> | null = null
  mockIPC((command) => {
    assert.equal(command, 'factory_reset')
    if (failure) {
      throw failure
    }
    return gate ? gate.then(() => response) : response
  })
  const { useNotificationStore } = await import('../../../stores/notificationStore')
  const { useFactoryReset } = await import('./usePlatformQueries')
  notifyManager.setNotifyFunction((callback) => act(callback))

  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

  beforeEach(() => {
    response = { ...complete }
    failure = null
    gate = null
    localStorage.setItem('retained-settings', 'keep-until-complete')
    useNotificationStore.getState().setNotificationsEnabled(true)
  })
  afterEach(async () => {
    await act(async () => cleanup())
    client.clear()
    notifications.clean()
  })
  after(async () => {
    notifyManager.setNotifyFunction((callback) => callback())
    await GlobalRegistrator.unregister()
  })

  for (const flag of ['caches_cleared', 'tokens_cleared', 'metrics_cleared'] as const) {
    test(`an incomplete ${flag} result preserves preferences and skips completion callbacks`, async (context) => {
      const notices = context.mock.method(notifications, 'show', () => 'reset-failure')
      const completed = context.mock.fn()
      response[flag] = false
      const { result } = renderHook(useFactoryReset, { wrapper })

      await act(async () => {
        await assert.rejects(result.current.mutateAsync(undefined, { onSuccess: completed }))
      })

      assert.equal(localStorage.getItem('retained-settings'), 'keep-until-complete')
      assert.equal(useNotificationStore.getState().notificationsEnabled, true)
      assert.equal(completed.mock.callCount(), 0)
      assert.equal(notices.mock.callCount(), 1)
      assert.equal(notices.mock.calls.at(0)?.arguments[0]?.color, 'red')
    })
  }

  test('backend deletion errors keep retry settings and never report completion', async (context) => {
    const notices = context.mock.method(notifications, 'show', () => 'reset-failure')
    const completed = context.mock.fn()
    failure = new Error('Keychain is locked')
    const { result } = renderHook(useFactoryReset, { wrapper })

    await act(async () => {
      await assert.rejects(result.current.mutateAsync(undefined, { onSuccess: completed }))
    })

    assert.equal(localStorage.getItem('retained-settings'), 'keep-until-complete')
    assert.equal(completed.mock.callCount(), 0)
    assert.equal(notices.mock.calls.at(0)?.arguments[0]?.color, 'red')
  })

  test('a complete reset clears preferences and reports completion', async (context) => {
    const notices = context.mock.method(notifications, 'show', () => 'reset-complete')
    const completed = context.mock.fn()
    const { result } = renderHook(useFactoryReset, { wrapper })

    await act(async () => {
      await result.current.mutateAsync(undefined, { onSuccess: completed })
    })

    assert.equal(localStorage.getItem('retained-settings'), null)
    assert.equal(useNotificationStore.getState().notificationsEnabled, false)
    assert.equal(completed.mock.callCount(), 1)
    assert.equal(notices.mock.calls.at(0)?.arguments[0]?.color, 'green')
  })

  test('the reset stays pending and keeps preferences until the backend answers', async () => {
    const release = Promise.withResolvers<void>()
    gate = release.promise
    const { result } = renderHook(useFactoryReset, { wrapper })
    let finished: Promise<unknown> = Promise.resolve()

    act(() => {
      finished = result.current.mutateAsync()
    })
    await waitFor(() => assert.equal(result.current.isPending, true))
    assert.equal(localStorage.getItem('retained-settings'), 'keep-until-complete')

    release.resolve()
    await act(async () => finished)
    assert.equal(localStorage.getItem('retained-settings'), null)
  })
}
