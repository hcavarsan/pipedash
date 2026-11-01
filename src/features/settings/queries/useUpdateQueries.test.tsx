import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import type { ReactNode } from 'react'

import type { UpdateStatus } from '../../../types/updater'
import { isRecord } from '../../../utils/isRecord'
import { updateStatusFixture } from '../fixtures/updateStatus'

const runtime = process.env.PIPEDASH_UPDATER_RUNTIME
GlobalRegistrator.register({ width: 390, height: 844 })
Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: runtime === 'ios' || runtime === 'android' ? runtime : 'linux' },
})
let status: UpdateStatus = { ...updateStatusFixture }
const calls: { command: string; args: unknown }[] = []
let getter: (() => Promise<UpdateStatus>) | undefined
let listenGate: Promise<void> | undefined
let rejectListener = false
mockIPC(
  (command) => {
    if (command === 'get_update_status') {
      return getter ? getter() : status
    }
    if (command === 'acknowledge_update_notice') {
      status = { ...status, revision: status.revision + 1, notice: null }
      return status
    }
    throw new Error(`Unexpected command: ${command}`)
  },
  { shouldMockEvents: true }
)

const internals: unknown = Reflect.get(window, '__TAURI_INTERNALS__')
assert.ok(isRecord(internals))
const dispatch = internals.invoke
assert.ok(typeof dispatch === 'function')
internals.invoke = async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  calls.push({ command, args })
  if (command === 'plugin:event|listen') {
    if (rejectListener) {
      throw new Error('Listener registration failed')
    }
    await listenGate
  }
  return dispatch(command, command === 'plugin:event|unlisten' ? { ...args, id: args?.eventId } : args)
}
if (runtime === 'browser') {
  Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
}
localStorage.setItem('platform-override', 'windows')

const { act, cleanup, render, renderHook, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { emit } = await import('@tauri-apps/api/event')
const { DesktopUpdateBridge } = await import('../../../components/DesktopUpdateBridge')
const { DesktopUpdates } = await import('../components/DesktopUpdates')
const { DesktopUpdateIndicator } = await import('../../../components/common/DesktopUpdateIndicator')
const { useAutomaticUpdates, useUpdateActions, useUpdateStatus, applyUpdateStatus } = await import('./useUpdateQueries')
const { queryKeys } = await import('../../../lib/queryKeys')
const { notifications } = await import('../../../lib/notifications')
const { DESKTOP_UPDATE_EVENT, DESKTOP_UPDATE_RESET_EVENT } = await import('../../../services/desktop')
const view = () => within(document.body)
let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  status = { ...updateStatusFixture }
  getter = undefined
  listenGate = undefined
  rejectListener = false
  calls.length = 0
  notifyManager.setNotifyFunction((callback) => act(callback))
})
afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

function Wrapper({ children }: { children: ReactNode }): ReactNode {
  return (
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <ModalsProvider>{children}</ModalsProvider>
      </QueryClientProvider>
    </MantineProvider>
  )
}

if (runtime) {
  test(`${runtime} appearance overrides never enable updater IPC`, async () => {
    render(
      <>
        <DesktopUpdateBridge />
        <DesktopUpdates />
        <DesktopUpdateIndicator />
      </>,
      { wrapper: Wrapper }
    )
    const hook = renderHook(
      () => ({ status: useUpdateStatus(), actions: useUpdateActions(), automatic: useAutomaticUpdates() }),
      { wrapper: Wrapper }
    )
    act(() => hook.result.current.actions.submit({ type: 'check' }))
    await act(async () => assert.rejects(hook.result.current.automatic.mutateAsync(true)))
    window.dispatchEvent(new Event('focus'))
    window.dispatchEvent(new Event(DESKTOP_UPDATE_RESET_EVENT))
    await act(async () => {})
    assert.deepEqual(
      calls.filter(({ command }) => command !== 'native_haptic'),
      []
    )
    assert.equal(view().queryByText('Updates'), null)
  })
} else {
  for (const mode of ['browser', 'ios', 'android']) {
    test(`updater stays off in an isolated ${mode} runtime`, () => {
      const result = spawnSync(process.execPath, ['test', '--isolate', import.meta.filename], {
        env: { ...process.env, PIPEDASH_UPDATER_RUNTIME: mode },
        encoding: 'utf8',
        timeout: 30000,
      })
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
    })
  }

  test('the local getter query cannot replace a newer event snapshot', async () => {
    const response = Promise.withResolvers<UpdateStatus>()
    getter = () => response.promise
    const hook = renderHook(useUpdateStatus, { wrapper: Wrapper })
    await waitFor(() => assert.equal(calls.filter((call) => call.command === 'get_update_status').length, 1))
    await act(async () =>
      applyUpdateStatus(client, { ...status, revision: 7, phase: 'ready', available_version: '0.8.1' })
    )
    await act(async () => response.resolve({ ...status, revision: 2 }))
    await waitFor(() => assert.equal(hook.result.current.isFetching, false))
    assert.equal(hook.result.current.data?.revision, 7)
    assert.equal(hook.result.current.data?.phase, 'ready')
    assert.equal(client.getQueryState(queryKeys.updates.status())?.dataUpdatedAt !== 0, true)
  })

  test('registration always reconciles even with an existing infinitely fresh snapshot', async () => {
    client.setQueryData(queryKeys.updates.status(), { ...status, revision: 1 })
    status = { ...status, revision: 4, phase: 'available', available_version: '0.8.1' }
    render(<DesktopUpdateBridge />, { wrapper: Wrapper })
    await waitFor(() => assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.revision, 4))
    const listen = calls.findIndex((call) => call.command === 'plugin:event|listen')
    const read = calls.findIndex((call) => call.command === 'get_update_status')
    assert.ok(listen >= 0 && read > listen)
  })

  test('events and command writes retain the highest revision during reconciliation', async () => {
    const response = Promise.withResolvers<UpdateStatus>()
    getter = () => response.promise
    render(<DesktopUpdateBridge />, { wrapper: Wrapper })
    await waitFor(() => assert.equal(client.getQueryData(queryKeys.updates.connection()), 'connected'))
    await act(async () =>
      emit(DESKTOP_UPDATE_EVENT, { ...status, revision: 8, phase: 'downloading', downloaded_bytes: 512 })
    )
    await act(async () => response.resolve({ ...status, revision: 3 }))
    await act(async () => applyUpdateStatus(client, { ...status, revision: 4 }))
    assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.revision, 8)
    assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.downloaded_bytes, 512)
  })

  test('a late resolving listener is disposed after unmount', async () => {
    const registered = Promise.withResolvers<void>()
    listenGate = registered.promise
    const mounted = render(<DesktopUpdateBridge />, { wrapper: Wrapper })
    await waitFor(() => assert.equal(calls.filter((call) => call.command === 'plugin:event|listen').length, 1))
    await act(async () => mounted.unmount())
    await act(async () => registered.resolve())
    await waitFor(() => assert.equal(calls.filter((call) => call.command === 'plugin:event|unlisten').length, 1))
    await act(async () => emit(DESKTOP_UPDATE_EVENT, { ...status, revision: 99 }))
    assert.notEqual(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.revision, 99)
  })

  test('listener registration failure blocks install until Retry connection succeeds', async () => {
    status = { ...status, phase: 'ready', available_version: '0.8.1', can_install: true }
    rejectListener = true
    render(
      <>
        <DesktopUpdateBridge />
        <DesktopUpdates />
      </>,
      { wrapper: Wrapper }
    )
    await waitFor(() => assert.ok(view().getByText("Couldn't receive update progress")))
    assert.equal(view().getByRole('button', { name: 'Install and restart' }).hasAttribute('disabled'), true)
    rejectListener = false
    const user = userEvent.setup({ document, delay: null })
    await user.click(view().getByRole('button', { name: 'Retry connection' }))
    await waitFor(() => assert.equal(client.getQueryData(queryKeys.updates.connection()), 'connected'))
    await waitFor(() =>
      assert.equal(view().getByRole('button', { name: 'Install and restart' }).hasAttribute('disabled'), false)
    )
    assert.equal(view().queryByText("Couldn't receive update progress"), null)
  })

  test('focus and factory-reset cache clearing reconcile local state without scheduling network commands', async () => {
    render(<DesktopUpdateBridge />, { wrapper: Wrapper })
    await waitFor(() => assert.equal(client.getQueryData(queryKeys.updates.connection()), 'connected'))
    const initialReads = calls.filter((call) => call.command === 'get_update_status').length
    status = { ...status, revision: 5 }
    await act(async () => window.dispatchEvent(new Event('focus')))
    await waitFor(() => assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.revision, 5))
    status = { ...updateStatusFixture, revision: 9, automatic_updates: true }
    await act(async () => {
      client.clear()
      window.dispatchEvent(new Event(DESKTOP_UPDATE_RESET_EVENT))
    })
    await waitFor(() => assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.revision, 9))
    assert.equal(client.getQueryData(queryKeys.updates.connection()), 'connected')
    assert.ok(calls.filter((call) => call.command === 'get_update_status').length > initialReads)
    assert.equal(
      calls.some((call) => call.command === 'check_for_updates' || call.command === 'download_update'),
      false
    )
  })

  test('the restart notice is displayed and acknowledged once for its exact version', async (context) => {
    status = { ...status, notice: { version: '0.8.1', outcome: 'updated' } }
    const notices = context.mock.method(notifications, 'show', () => 'updater-notice')
    render(<DesktopUpdateBridge />, { wrapper: Wrapper })
    await waitFor(() => assert.equal(calls.filter((call) => call.command === 'acknowledge_update_notice').length, 1))
    assert.deepEqual(calls.find((call) => call.command === 'acknowledge_update_notice')?.args, { version: '0.8.1' })
    assert.equal(notices.mock.calls[0]?.arguments[0]?.title, 'Updated to 0.8.1')
    await act(async () =>
      emit(DESKTOP_UPDATE_EVENT, { ...status, revision: 10, notice: { version: '0.8.1', outcome: 'updated' } })
    )
    assert.equal(notices.mock.callCount(), 1)
    assert.equal(calls.filter((call) => call.command === 'acknowledge_update_notice').length, 1)
  })

  test('root progress survives child removal and cannot be dismissed before native failure', async () => {
    const user = userEvent.setup({ document, delay: null })
    const mounted = render(
      <>
        <DesktopUpdateBridge />
        <DesktopUpdates />
      </>,
      { wrapper: Wrapper }
    )
    await waitFor(() => assert.equal(client.getQueryData(queryKeys.updates.connection()), 'connected'))
    await act(async () => emit(DESKTOP_UPDATE_EVENT, { ...status, revision: 2, phase: 'preparing' }))
    await waitFor(() => assert.ok(view().getByRole('dialog', { name: 'Updating Pipedash' })))
    mounted.rerender(<DesktopUpdateBridge />)
    await user.keyboard('{Escape}')
    assert.ok(view().getByRole('dialog', { name: 'Updating Pipedash' }))
    assert.equal(view().queryByRole('button', { name: 'Close dialog' }), null)
    await act(async () =>
      emit(DESKTOP_UPDATE_EVENT, {
        ...status,
        revision: 3,
        phase: 'ready',
        error: { stage: 'install', code: 'UPDATE_INSTALL_FAILED', message: 'Permission denied' },
      })
    )
    await waitFor(() => assert.equal(client.getQueryData<UpdateStatus>(queryKeys.updates.status())?.phase, 'ready'))
    await waitFor(() => assert.equal(view().queryByRole('dialog', { name: 'Updating Pipedash' }), null))
  })
}
