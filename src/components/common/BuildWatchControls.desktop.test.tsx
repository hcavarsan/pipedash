import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { BuildWatchSnapshot, BuildWatchStatus, DesktopStatus } from '../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })
const snapshot: BuildWatchSnapshot = {
  sessionId: 'desktop-watch',
  pipelineId: 'jenkins__1__menu-01',
  pipelineName: 'menu-01',
  providerName: 'Menu smoke',
  providerType: 'jenkins',
  runNumber: 42,
  branch: 'main',
  status: 'running',
  startedAt: 1792886400,
  concludedAt: null,
  updatedAt: 1792886400,
  monitoring: 'watching',
  url: 'http://localhost/job/menu-01/42',
}
const available = (): BuildWatchStatus => ({ supported: true, available: true, watch: null, reason: null })
let status = available()
let change: ((status: BuildWatchStatus) => void) | undefined
let gate: PromiseWithResolvers<void> | null = null
const commands: string[] = []
const callbacks = new Map<number, (event: unknown) => void>()
let next = 0

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      callbacks.set(++next, callback)
      return next
    },
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      commands.push(command)
      if (command === 'plugin:event|listen') {
        const callback = callbacks.get(Number(args.handler))
        change = (payload) => callback?.({ event: 'build-watch-changed', payload })
        await gate?.promise
        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        return
      }
      if (command === 'get_build_watch') {
        return status
      }
      if (command === 'start_build_watch') {
        assert.deepEqual(args, { pipelineId: snapshot.pipelineId, runNumber: 42 })
        status = { ...available(), watch: { ...snapshot } }
        return status
      }
      if (command === 'stop_build_watch') {
        status = { ...status, watch: { ...snapshot, monitoring: 'stopped' } }
        return status
      }
      throw new Error(`Unexpected command: ${command}`)
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { BuildWatchControls, BuildWatchSection } = await import('./BuildWatchControls')
const { useBuildWatchLifecycle } = await import('../../queries/useBuildWatchQueries')
const { queryKeys } = await import('../../lib/queryKeys')
let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const view = () => within(document.body)

function Screen() {
  useBuildWatchLifecycle()
  return (
    <>
      <BuildWatchSection onViewRun={() => {}} />
      <BuildWatchControls pipelineId={snapshot.pipelineId} runNumber={42} status="running" />
    </>
  )
}

function mount() {
  const desktop: DesktopStatus = {
    revision: 1,
    settings: { show_tray: false, keep_running_on_close: false, notifications_enabled: false },
    tray_state: 'off',
    ready: true,
    notification_preference_imported: true,
    error: null,
  }
  client.setQueryData(['desktop', 'status'], desktop)
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Screen />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  commands.length = 0
  status = available()
  change = undefined
  gate = null
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

test('desktop watches subscribe before fetching and stay available with the tray off', async () => {
  gate = Promise.withResolvers<void>()
  const user = userEvent.setup({ document, delay: null })
  mount()
  await act(async () => {})
  assert.equal(commands.includes('get_build_watch'), false)
  await act(async () => gate?.resolve())
  const button = await view().findByRole('button', { name: 'Watch run' })
  assert.ok(button instanceof HTMLButtonElement)
  await waitFor(() => assert.equal(button.disabled, false))
  await user.click(button)
  await waitFor(() =>
    assert.equal(client.getQueryData<BuildWatchStatus>(queryKeys.buildWatch.status())?.watch?.runNumber, 42)
  )
  assert.ok(view().getAllByRole('link', { name: 'General settings' }).length > 0)
  assert.ok(view().getByText(/Closing the window keeps watching only when background mode is on/))
})

test('desktop push changes update cached watch state without a native fetch or five-second UI poll', async (context) => {
  context.mock.timers.enable({ apis: ['setInterval'] })
  status = { ...available(), watch: { ...snapshot } }
  mount()
  await view().findByRole('region', { name: 'Watching' })
  await waitFor(() => assert.equal(client.isFetching(), 0))
  const reads = commands.filter((command) => command === 'get_build_watch').length
  const completed: BuildWatchStatus = {
    ...available(),
    watch: { ...snapshot, monitoring: 'completed', status: 'success' },
  }
  act(() => change?.(completed))
  await waitFor(() => assert.deepEqual(client.getQueryData(queryKeys.buildWatch.status()), completed))
  assert.equal(view().queryByRole('button', { name: 'Stop watching' }), null)
  await act(async () => {
    context.mock.timers.tick(5100)
  })
  assert.equal(commands.filter((command) => command === 'get_build_watch').length, reads)
  status = { ...completed, watch: { ...snapshot, monitoring: 'interrupted' } }
  act(() => window.dispatchEvent(new Event('focus')))
  await waitFor(() =>
    assert.equal(client.getQueryData<BuildWatchStatus>(queryKeys.buildWatch.status())?.watch?.monitoring, 'interrupted')
  )
  assert.equal(commands.filter((command) => command === 'get_build_watch').length, reads + 1)
})

test('desktop unavailability uses General settings without invoking mobile app settings', async () => {
  status = { ...available(), available: false, reason: 'Pipedash is locked.' }
  const user = userEvent.setup({ document, delay: null })
  mount()
  await view().findByText('Pipedash is locked.')
  await user.click(view().getByRole('button', { name: 'Open Settings' }))
  assert.equal(commands.includes('native_open_settings'), false)
  assert.equal(commands.includes('start_build_watch'), false)
})
