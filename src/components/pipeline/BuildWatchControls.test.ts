import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment } from 'react'

import type { BuildWatchSnapshot, BuildWatchStatus } from '../../types'

if (process.env.PIPEDASH_BUILD_WATCH_TEST !== '1') {
  test('native watch behavior stays isolated from shared platform caches', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_BUILD_WATCH_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 1200, height: 900 })

  const snapshot: BuildWatchSnapshot = {
    sessionId: 'session-one',
    pipelineId: 'github__1__owner/repo/build.yml',
    pipelineName: 'Build',
    providerName: 'GitHub',
    providerType: 'github',
    runNumber: 42,
    branch: 'main',
    status: 'pending',
    startedAt: 1792886400,
    concludedAt: null,
    updatedAt: 1792886400,
    monitoring: 'watching',
    url: 'pipedash://pipelines/github__1__owner%2Frepo%2Fbuild.yml/runs/42',
  }
  const available = (): BuildWatchStatus => ({ supported: true, available: true, reason: null, watch: null })
  let nativeStatus = available()
  let failure: Error | null = null
  let startFailure: Error | null = null
  let stopFailure: Error | null = null
  let startGate: Promise<void> | null = null
  let nativeChange: (() => void) | undefined
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = []
  const os = { platform: 'ios' }
  const callbacks = new Map<number, (event: unknown) => void>()
  let nextCallback = 0

  Object.assign(window, {
    __TAURI_OS_PLUGIN_INTERNALS__: os,
    __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
    __TAURI_INTERNALS__: {
      transformCallback: (callback: (event: unknown) => void) => {
        const id = ++nextCallback
        callbacks.set(id, callback)
        return id
      },
      invoke: async (command: string, args?: Record<string, unknown>) => {
        calls.push({ command, args })
        switch (command) {
          case 'get_build_watch':
            if (failure) {
              throw failure
            }
            return nativeStatus
          case 'start_build_watch':
            if (startFailure) {
              throw startFailure
            }
            await startGate
            nativeStatus = { ...available(), watch: { ...snapshot } }
            return nativeStatus
          case 'stop_build_watch':
            if (stopFailure) {
              throw stopFailure
            }
            nativeStatus = { ...nativeStatus, watch: { ...snapshot, monitoring: 'stopped' } }
            return nativeStatus
          case 'plugin:event|listen': {
            assert.equal(args?.event, 'build-watch-changed')
            const callback = typeof args?.handler === 'number' ? callbacks.get(args.handler) : undefined
            assert.ok(callback)
            nativeChange = () => callback({ event: 'build-watch-changed', id: 1, payload: null })
            return 1
          }
          case 'plugin:event|unlisten':
          case 'native_open_settings':
            return
          default:
            throw new Error(`Unexpected command: ${command}`)
        }
      },
    },
  })

  const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
  const { MantineProvider } = await import('@mantine/core')
  const { notifications } = await import('@mantine/notifications')
  const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { useBuildWatchLifecycle } = await import('../../queries/useBuildWatchQueries')
  const { BuildWatchControls, BuildWatchPanel } = await import('./BuildWatchControls')
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const view = () => within(document.body)

  function Lifecycle() {
    useBuildWatchLifecycle()
    return null
  }

  function mount(
    options: { secondRun?: boolean; lifecycle?: boolean; status?: 'pending' | 'running' | 'success' } = {}
  ) {
    const opened: Array<[string, number]> = []
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(
            Fragment,
            null,
            options.lifecycle && createElement(Lifecycle),
            createElement(BuildWatchPanel, { onViewRun: (id, number) => opened.push([id, number]) }),
            createElement(BuildWatchControls, {
              pipelineId: snapshot.pipelineId,
              runNumber: 42,
              status: options.status ?? 'pending',
            }),
            options.secondRun &&
              createElement(BuildWatchControls, { pipelineId: snapshot.pipelineId, runNumber: 43, status: 'running' })
          )
        )
      )
    )
    return opened
  }

  notifyManager.setNotifyFunction((callback) => act(callback))

  afterEach(async () => {
    await act(async () => cleanup())
    client.clear()
    notifications.clean()
    calls.length = 0
    nativeStatus = available()
    failure = null
    startFailure = null
    stopFailure = null
    startGate = null
    nativeChange = undefined
    callbacks.clear()
    os.platform = 'ios'
  })

  after(() => {
    notifyManager.setNotifyFunction((callback) => callback())
    GlobalRegistrator.unregister()
  })

  test('pending and running controls share one pending start and send only actual run identity', async () => {
    const gate = Promise.withResolvers<void>()
    startGate = gate.promise
    mount({ secondRun: true })
    const buttons = await view().findAllByRole('button', { name: 'Watch run' })
    await waitFor(() => assert.ok(buttons.every((button) => !(button as HTMLButtonElement).disabled)))
    act(() => {
      fireEvent.click(buttons[0])
      fireEvent.click(buttons[0])
      fireEvent.click(buttons[1])
    })
    await waitFor(() => assert.equal(calls.filter((call) => call.command === 'start_build_watch').length, 1))
    assert.deepEqual(calls.find((call) => call.command === 'start_build_watch')?.args, {
      pipelineId: snapshot.pipelineId,
      runNumber: 42,
    })
    await act(async () => gate.resolve())
    await view().findByText('Watching run')
    assert.equal((view().getByRole('button', { name: 'Watch run' }) as HTMLButtonElement).disabled, true)
    assert.match(document.body.textContent ?? '', /Stop the current watch before watching another run/)
  })

  test('stop ends monitoring without cancelling the provider and viewing keeps exact run identity', async () => {
    nativeStatus = { ...available(), watch: { ...snapshot } }
    const opened = mount()
    fireEvent.click(await view().findByRole('button', { name: 'View run #42' }))
    assert.deepEqual(opened, [[snapshot.pipelineId, 42]])
    fireEvent.click(view().getAllByRole('button', { name: 'Stop watching' })[0])
    await view().findByText('Watch stopped')
    assert.equal(calls.filter((call) => call.command === 'stop_build_watch').length, 1)
    assert.equal(
      calls.some((call) => call.command.includes('cancel')),
      false
    )
    assert.match(document.body.textContent ?? '', /provider run was not cancelled/)
  })

  test('native interruption reconciles on lifecycle return without automatically restarting', async () => {
    nativeStatus = { ...available(), watch: { ...snapshot } }
    mount({ lifecycle: true })
    await view().findByText('Watching run')
    await waitFor(() => assert.equal(typeof nativeChange, 'function'))
    nativeStatus = { ...available(), watch: { ...snapshot, monitoring: 'interrupted' } }
    act(() => nativeChange?.())
    await view().findByText('Watch interrupted')
    assert.equal(
      calls.some((call) => call.command === 'start_build_watch'),
      false
    )
    nativeStatus = {
      ...available(),
      watch: { ...snapshot, monitoring: 'completed', status: 'success', concludedAt: snapshot.updatedAt + 10 },
    }
    act(() => window.dispatchEvent(new Event('focus')))
    await view().findByText('Watch completed')
    assert.match(document.body.textContent ?? '', /Success/)
  })

  test('supported iOS explains disabled native permissions and opens Settings without starting', async () => {
    nativeStatus = { ...available(), available: false, reason: 'Live Activities are disabled in iOS Settings.' }
    mount()
    await view().findByText(nativeStatus.reason ?? '')
    assert.equal(view().queryByText('Watch unavailable'), null)
    assert.ok(view().getAllByText(nativeStatus.reason ?? '').length > 0)
    assert.equal((view().getByRole('button', { name: 'Watch run' }) as HTMLButtonElement).disabled, true)
    fireEvent.click(view().getByRole('button', { name: 'Open Settings' }))
    await waitFor(() =>
      assert.equal(
        calls.some((call) => call.command === 'native_open_settings'),
        true
      )
    )
    assert.equal(
      calls.some((call) => call.command === 'start_build_watch'),
      false
    )
  })

  test('unsupported native versions hide Watch controls', async () => {
    nativeStatus = { supported: false, available: false, reason: 'Requires iOS 26.', watch: null }
    mount()
    await waitFor(() =>
      assert.equal(
        calls.some((call) => call.command === 'get_build_watch'),
        true
      )
    )
    await waitFor(() => assert.equal(client.isFetching(), 0))
    assert.equal(view().queryByRole('button', { name: 'Watch run' }), null)
    assert.equal(view().queryByText('Watch unavailable'), null)
  })

  test('non-iOS shells never call Watch commands or show Watch controls', () => {
    os.platform = 'android'
    mount({ lifecycle: true })
    assert.equal(calls.length, 0)
    assert.equal(view().queryByRole('button', { name: 'Watch run' }), null)
  })

  test('status errors remain visible and can be reconciled with an explicit refresh', async () => {
    failure = new Error('Native status unavailable')
    mount()
    await view().findByText("Couldn't refresh Watch")
    assert.equal(view().queryByRole('button', { name: 'Watch run' }), null)
    failure = null
    fireEvent.click(view().getByRole('button', { name: 'Refresh Watch' }))
    await view().findByRole('button', { name: 'Watch run' })
  })

  test('terminal provider runs cannot start a watch', async () => {
    mount({ status: 'success' })
    await waitFor(() =>
      assert.equal(
        calls.some((call) => call.command === 'get_build_watch'),
        true
      )
    )
    await waitFor(() => assert.equal(client.isFetching(), 0))
    assert.equal(view().queryByRole('button', { name: 'Watch run' }), null)
  })

  test('failed start stays visible as an error without claiming the run is watched', async (context) => {
    const notices = context.mock.method(notifications, 'show', () => 'watch-error')
    startFailure = new Error('Background execution is disabled')
    mount()
    const button = await view().findByRole('button', { name: 'Watch run' })
    await waitFor(() => assert.equal((button as HTMLButtonElement).disabled, false))
    fireEvent.click(button)
    await waitFor(() => assert.equal(notices.mock.calls.length, 1))
    assert.equal(view().queryByText('Watching run'), null)
    await waitFor(() => assert.equal((button as HTMLButtonElement).disabled, false))
  })

  test('failed stop preserves authoritative watch state and reports the failure', async (context) => {
    const notices = context.mock.method(notifications, 'show', () => 'watch-error')
    nativeStatus = { ...available(), watch: { ...snapshot } }
    stopFailure = new Error('Native stop failed')
    mount()
    fireEvent.click((await view().findAllByRole('button', { name: 'Stop watching' }))[0])
    await waitFor(() => assert.equal(notices.mock.calls.length, 1))
    assert.ok(view().getByText('Watching run'))
    assert.equal(view().queryByText('Watch stopped'), null)
  })
}
