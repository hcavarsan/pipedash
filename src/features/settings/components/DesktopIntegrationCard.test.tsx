import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { DesktopSettingsPatch, DesktopStatus } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })
let status: DesktopStatus = {
  revision: 0,
  settings: { show_tray: false, keep_running_on_close: false, notifications_enabled: false },
  tray_state: 'off',
  ready: true,
  notification_preference_imported: true,
  error: null,
}
const initial = structuredClone(status)
const callbacks = new Map<number, (event: unknown) => void>()
let handler = 0
const patches: DesktopSettingsPatch[] = []
let failure: Error | null = null
let saveGate: Promise<void> | null = null
let testsSubmitted = 0
const os = { platform: 'linux' }

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: os,
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      callbacks.set(++handler, callback)
      return handler
    },
    invoke: async (command: string, args: { patch?: DesktopSettingsPatch; handler?: number } = {}) => {
      if (command === 'plugin:event|listen') {
        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        return
      }
      if (command === 'get_desktop_status') {
        return structuredClone(status)
      }
      if (command === 'update_desktop_settings') {
        const patch = args.patch ?? {}
        patches.push(patch)
        await saveGate
        if (failure) {
          throw failure
        }
        status = { ...status, revision: status.revision + 1, error: null, settings: { ...status.settings, ...patch } }
        status.tray_state = status.settings.show_tray ? 'ready' : 'off'
        if (!status.settings.show_tray) {
          status.settings.keep_running_on_close = false
        }
        return structuredClone(status)
      }
      if (command === 'test_desktop_notification') {
        testsSubmitted += 1
        if (failure) {
          throw failure
        }
        return
      }
      if (command === 'mark_desktop_notifications_read') {
        return { revision: 2, items: [] }
      }
      if (command === 'list_providers') {
        return []
      }
      throw new Error(`Unexpected native command: ${command}`)
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { useDesktopLifecycle } = await import('../queries/useDesktopQueries')
const { DesktopIntegrationCard } = await import('./DesktopIntegrationCard')
const { DesktopNotificationsCard } = await import('../../notifications/components/DesktopNotificationsCard')
const { NotificationDetailsModal } = await import('../../notifications/components/NotificationDetailsModal')
const { AppRoutes } = await import('../../../AppRoutes')
const { useNotificationCenterStore } = await import('../../../stores/notificationCenterStore')
const { usePipelineViewStore } = await import('../../pipelines/stores/pipelineViewStore')
const { useModalStore } = await import('../../../stores/modalStore')
const { queryKeys } = await import('../../../lib/queryKeys')
const { desktopNotificationsKey } = await import('../../notifications/queries/useDesktopNotifications')
const { notifications } = await import('../../../lib/notifications')
const { setPlatformOverride } = await import('../../../utils/platform')
let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const view = () => within(document.body)

function Screen({ page = 'general' }: { page?: 'general' | 'notifications' | 'route' | 'details' }) {
  useDesktopLifecycle()
  const location = useLocation()
  return (
    <>
      {page === 'general' && <DesktopIntegrationCard />}
      {page === 'notifications' && <DesktopNotificationsCard />}
      {page === 'details' && <NotificationDetailsModal />}
      {page === 'route' && (
        <AppRoutes
          providers={[]}
          pipelines={[]}
          loading={false}
          error={null}
          onRetry={async () => {}}
          onBack={() => {}}
          onViewHistory={() => {}}
          onViewMetrics={() => {}}
          onViewRun={() => {}}
          onTrigger={() => {}}
        />
      )}
      <output aria-label="Location">
        {location.pathname}
        {location.search}
      </output>
    </>
  )
}

function mount(page: 'general' | 'notifications' | 'route' | 'details' = 'general') {
  client.setQueryData(queryKeys.providers.list(), [])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[page === 'route' ? '/notifications' : '/settings/general']}>
          <Screen page={page} />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  status = structuredClone(initial)
  os.platform = 'linux'
  failure = null
  saveGate = null
  testsSubmitted = 0
  patches.length = 0
  callbacks.clear()
  setPlatformOverride(null)
  useNotificationCenterStore.setState({ items: [], selection: null })
  useModalStore.setState(useModalStore.getInitialState(), true)
  usePipelineViewStore.setState(usePipelineViewStore.getInitialState(), true)
})

after(() => GlobalRegistrator.unregister())

test('native OS labels ignore appearance overrides and background mode waits for a confirmed tray', async () => {
  setPlatformOverride('macos')
  const user = userEvent.setup({ document, delay: null })
  mount()
  const tray = await view().findByRole('switch', { name: 'Show in system tray' })
  const background = view().getByRole('switch', { name: 'Keep running when the window closes' })
  assert.ok(tray instanceof HTMLInputElement && background instanceof HTMLInputElement)
  await waitFor(() => assert.equal(tray.disabled, false))
  assert.equal(tray.checked, false)
  assert.equal(background.disabled, true)
  assert.ok(view().getByText(/Your desktop may need a tray extension/))
  const gate = Promise.withResolvers<void>()
  saveGate = gate.promise
  await user.click(tray)
  await waitFor(() => assert.equal(tray.disabled, true))
  assert.equal(tray.checked, false)
  assert.equal(background.disabled, true)
  await user.click(tray)
  assert.equal(patches.length, 1)
  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(tray.checked, true))
  assert.equal(background.disabled, false)
  await user.click(background)
  await waitFor(() => assert.equal(background.checked, true))
  assert.deepEqual(patches, [{ show_tray: true }, { keep_running_on_close: true }])
})

test('a failed tray save keeps the confirmed settings and exposes retry', async () => {
  const user = userEvent.setup({ document, delay: null })
  mount()
  const tray = await view().findByRole('switch', { name: 'Show in system tray' })
  assert.ok(tray instanceof HTMLInputElement)
  await waitFor(() => assert.equal(tray.disabled, false))
  failure = new Error('Tray creation failed')
  await user.click(tray)
  await view().findByText('Tray creation failed')
  assert.equal(tray.checked, false)
  failure = null
  await user.click(view().getByRole('button', { name: /retry|try again/i }))
  await waitFor(() => assert.equal(view().queryByText('Tray creation failed'), null))
  assert.deepEqual(patches, [{ show_tray: true }, {}])
})

test('tray errors remain visible and Retry submits an empty patch', async () => {
  status = { ...status, error: 'Tray unavailable', tray_state: 'error' }
  const user = userEvent.setup({ document, delay: null })
  mount()
  await view().findByText('Tray unavailable')
  const background = view().getByRole('switch', { name: 'Keep running when the window closes' })
  assert.ok(background instanceof HTMLInputElement && background.disabled)
  await user.click(view().getByRole('button', { name: /retry|try again/i }))
  await waitFor(() => assert.equal(view().queryByText('Tray unavailable'), null))
  assert.deepEqual(patches, [{}])
})

test('settings routes reuse the pinned editor and notification settings', async () => {
  const user = userEvent.setup({ document, delay: null })
  mount()
  await user.click(view().getByRole('button', { name: 'Manage pinned pipelines' }))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/pipelines?view=pinned')
  assert.equal(usePipelineViewStore.getState().editor.viewId, 'pinned')
  await user.click(view().getByRole('button', { name: 'Notification settings' }))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/settings/notifications')
})

test('notification preference uses native settings and test submission does not claim delivery', async (context) => {
  const shown = context.mock.method(notifications, 'show', () => 'desktop-test')
  const user = userEvent.setup({ document, delay: null })
  mount('notifications')
  const toggle = await view().findByRole('switch', { name: 'System notifications' })
  const submit = view().getByRole('button', { name: 'Send test notification' })
  assert.ok(toggle instanceof HTMLInputElement && submit instanceof HTMLButtonElement)
  await waitFor(() => assert.equal(toggle.disabled, false))
  assert.equal(submit.disabled, true)
  await user.click(toggle)
  await waitFor(() => assert.equal(submit.disabled, false))
  assert.deepEqual(patches, [{ notifications_enabled: true }])
  await user.click(submit)
  await waitFor(() => assert.equal(testsSubmitted, 1))
  assert.ok(shown.mock.calls.some((call) => call.arguments[0]?.title === 'Notification submitted'))
  assert.ok(view().getByText(/Do Not Disturb can suppress display/))
})

for (const [platform, guidance] of [
  ['macos', 'Allow Pipedash in System Settings > Notifications'],
  ['windows', 'Allow Pipedash in Settings > System > Notifications'],
] as const) {
  test(`${platform} notification guidance reports OS constraints without a permission badge`, async () => {
    os.platform = platform
    mount('notifications')
    await view().findByRole('switch', { name: 'System notifications' })
    assert.ok(view().getByText(`${guidance}. Do Not Disturb can suppress display.`))
    assert.equal(view().queryByText(/permission granted/i), null)
    if (platform === 'windows') {
      assert.ok(view().getByText(/Windows Action Center cannot reopen Pipedash/))
    }
  })
}

test('the desktop notifications route renders the shared list and header', async () => {
  mount('route')
  await view().findByRole('heading', { name: 'Notifications' })
  assert.ok(await view().findByText("You're all caught up"))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/notifications')
})

test('watched notification details open the exact run instead of arbitrary history', async () => {
  client.setQueryData(desktopNotificationsKey, {
    revision: 1,
    items: [
      {
        key: 'watch:one',
        at: Date.now(),
        read: true,
        tone: 'success',
        level: null,
        title: 'Watched run #42 succeeded',
        body: 'Build',
        providerIds: [],
        pipelineId: 'github__1__owner/repo%file#日本語',
        runNumber: 42,
        link: null,
      },
    ],
  })
  useNotificationCenterStore.setState({ selection: { type: 'notification', key: 'watch:one' } })
  const user = userEvent.setup({ document, delay: null })
  mount('details')
  await user.click(await view().findByRole('button', { name: 'Open run' }))
  assert.deepEqual(useModalStore.getState().logsModal, {
    open: true,
    pipelineId: 'github__1__owner/repo%file#日本語',
    runNumber: 42,
  })
  assert.equal(useNotificationCenterStore.getState().selection, null)
})
