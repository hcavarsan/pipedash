import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactElement } from 'react'

import { pipelineFixture, providerSummaryFixture } from '../../../testing/fixtures'
import type { NotificationPreferenceSnapshot, PreferenceChange } from '../../../types/notification_preferences'
import type { NotificationPreferences } from '../../../types/notifications'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: { invoke: async () => null },
})

const { cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { queryKeys } = await import('../../../lib/queryKeys')
const { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } = await import('../../../stores/notificationStore')
const { NotificationPreferencesSection } = await import('./NotificationPreferencesSection')
const { PipelineNotifyMenu } = await import('./PipelineNotifyMenu')
const { PipelineNotifyPolicy } = await import('./PipelineNotifyPolicy')
const { notifications } = await import('../../../lib/notifications')
const { changePreference, refreshPreferenceStates } = await import('../queries/changePreference')

const view = () => within(document.body)
const pipeline = pipelineFixture({ id: 'stub__1__acme/api__ci.yml', name: 'API CI', repository: 'acme/api' })
const changes: PreferenceChange[] = []
const currents: (NotificationPreferences | undefined)[] = []
let reply: NotificationPreferenceSnapshot
let respond: (current: NotificationPreferences | undefined) => Promise<NotificationPreferenceSnapshot>

function mount(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  client.setQueryData(queryKeys.pipelines.list(), [pipeline])
  client.setQueryData(queryKeys.providers.list(), [providerSummaryFixture({ id: 1, name: 'Stub' })])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>{ui}</QueryClientProvider>
    </MantineProvider>
  )
}

const snapshot = (
  pipelineStates: NotificationPreferenceSnapshot['pipeline_states'] = {}
): NotificationPreferenceSnapshot => ({
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
  pipeline_states: pipelineStates,
})

function idle(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>()
  setTimeout(resolve, 20)

  return promise
}

beforeEach(() => {
  changes.length = 0
  currents.length = 0
  reply = snapshot()
  respond = async () => reply
  const change = async (next: PreferenceChange, current?: NotificationPreferences) => {
    changes.push(next)
    currents.push(current)

    return respond(current)
  }

  mock.method(apiService, 'changeNotificationPreference', change)
  mock.method(tauriService, 'changeNotificationPreference', change)
})

afterEach(() => {
  cleanup()
  mock.restoreAll()
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES, pipelineStates: {} })
})
after(() => GlobalRegistrator.unregister())

test('the pipeline menu button names the current state', () => {
  const states = [
    { state: undefined, label: 'Notifications' },
    { state: 'watching', label: 'Watching' },
    { state: 'pending', label: 'Notify on finish' },
    { state: 'muted', label: 'Muted' },
  ] as const

  for (const { state, label } of states) {
    useNotificationStore.setState({ pipelineStates: state ? { [pipeline.id]: state } : {} })
    mount(<PipelineNotifyMenu pipeline={pipeline} />)
    assert.ok(view().getByRole('button', { name: `${label}, notification options` }))
    cleanup()
  }
})

test('the pipeline menu is one policy choice that marks the current option and sends the chosen change', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'running' }} />)

  await user.click(view().getByRole('button', { name: /notification options/ }))
  const options = await view().findAllByRole('menuitem')
  assert.deepEqual(
    options.map((option) => [option.textContent, option.getAttribute('aria-current')]),
    [
      ['Follow notification settings', 'true'],
      ['Notify when this run finishes', null],
      ['Watch all runs', null],
      ['Mute notifications', null],
    ]
  )

  reply = snapshot({ [pipeline.id]: 'watching' })
  await user.click(view().getByRole('menuitem', { name: 'Watch all runs' }))
  await waitFor(() =>
    assert.deepEqual(changes, [{ type: 'pipeline_watched', pipeline_id: pipeline.id, watched: true }])
  )

  await user.click(await view().findByRole('button', { name: 'Watching, notification options' }))
  const watching = await view().findByRole('menuitem', { name: 'Watch all runs' })
  assert.equal(watching.getAttribute('aria-current'), 'true')
})

test('switching between policies sends exactly one change', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'success' }} />)

  reply = snapshot({ [pipeline.id]: 'pending' })
  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Notify when the next run finishes' }))
  await waitFor(() => assert.deepEqual(changes.at(-1), { type: 'watch_next_finish', pipeline_id: pipeline.id }))

  reply = snapshot({ [pipeline.id]: 'muted' })
  await user.click(await view().findByRole('button', { name: 'Notify on finish, notification options' }))
  await user.click(await view().findByRole('menuitem', { name: 'Mute notifications' }))
  await waitFor(() =>
    assert.deepEqual(changes.slice(-2), [
      { type: 'watch_next_finish', pipeline_id: pipeline.id },
      { type: 'pipeline_muted', pipeline_id: pipeline.id, muted: true },
    ])
  )

  reply = snapshot()
  await user.click(await view().findByRole('button', { name: 'Muted, notification options' }))
  await user.click(await view().findByRole('menuitem', { name: 'Follow notification settings' }))
  await waitFor(() =>
    assert.deepEqual(changes.at(-1), { type: 'pipeline_muted', pipeline_id: pipeline.id, muted: false })
  )
})

test('the notifications dialog offers the same choice and closes after it is saved', async () => {
  const user = userEvent.setup({ document })
  const done = mock.fn()
  mount(<PipelineNotifyPolicy pipeline={{ ...pipeline, status: 'running' }} onDone={done} />)

  const group = view().getByRole('radiogroup')
  assert.equal(within(group).getAllByRole('radio').length, 4)

  reply = snapshot({ [pipeline.id]: 'muted' })
  await user.click(within(group).getByRole('radio', { name: /Mute notifications/ }))
  await waitFor(() => assert.equal(done.mock.callCount(), 1))
  assert.deepEqual(changes, [{ type: 'pipeline_muted', pipeline_id: pipeline.id, muted: true }])
})

test('a failed policy change keeps the previous choice and leaves the dialog open for another try', async () => {
  const user = userEvent.setup({ document })
  const done = mock.fn()
  const shown = mock.method(notifications, 'show')
  useNotificationStore.setState({ pipelineStates: { [pipeline.id]: 'muted' } })
  respond = async () => {
    throw new Error('Host unreachable')
  }
  mount(<PipelineNotifyPolicy pipeline={{ ...pipeline, status: 'running' }} onDone={done} />)

  await user.click(view().getByRole('radio', { name: /Watch all runs/ }))
  await waitFor(() => assert.ok(shown.mock.callCount() > 0))

  const muted = view().getByRole('radio', { name: /Mute notifications/ })
  assert.ok(muted instanceof HTMLInputElement)
  await waitFor(() => assert.deepEqual([muted.checked, muted.disabled], [true, false]))
  assert.deepEqual(changes, [{ type: 'pipeline_watched', pipeline_id: pipeline.id, watched: true }])
  assert.equal(done.mock.callCount(), 0)

  respond = async () => snapshot({ [pipeline.id]: 'watching' })
  await user.click(view().getByRole('radio', { name: /Watch all runs/ }))
  await waitFor(() => assert.equal(done.mock.callCount(), 1))
})

test('settings list watched pipelines and pending requests from the backend states, and each can be undone', async () => {
  const user = userEvent.setup({ document })
  const other = 'stub__1__acme/web__deploy.yml'
  useNotificationStore.setState({ pipelineStates: { [pipeline.id]: 'watching', [other]: 'pending' } })
  reply = snapshot({ [other]: 'pending' })
  mount(<NotificationPreferencesSection />)

  assert.ok(view().getByText('API CI'))
  assert.ok(view().getByText(/Next finish only/))

  await user.click(view().getByRole('button', { name: 'Stop watching API CI' }))
  await waitFor(() =>
    assert.deepEqual(changes.at(-1), { type: 'pipeline_watched', pipeline_id: pipeline.id, watched: false })
  )

  await user.click(view().getByRole('button', { name: `Cancel finish notification for ${other}` }))
  await waitFor(() => assert.deepEqual(changes.at(-1), { type: 'cancel_next_finish', pipeline_id: other }))
})

test('a request that already fired is shown as the default state in the menu and in settings', () => {
  mount(
    <>
      <PipelineNotifyMenu pipeline={{ ...pipeline, status: 'success' }} />
      <NotificationPreferencesSection />
    </>
  )

  assert.ok(view().getByRole('button', { name: 'Notifications, notification options' }))
  assert.equal(view().queryByText(/Next finish only/), null)
})

const menuActions = [
  { item: 'Watch all runs', toast: 'Watching pipeline' },
  { item: 'Notify when this run finishes', toast: "You'll be notified when API CI finishes" },
  { item: 'Mute notifications', toast: 'Pipeline muted' },
] as const

for (const { item, toast } of menuActions) {
  test(`"${item}" confirms only after the change is saved`, async () => {
    const shown = mock.method(notifications, 'show')
    const saved = Promise.withResolvers<NotificationPreferenceSnapshot>()
    respond = () => saved.promise
    const user = userEvent.setup({ document })
    mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'running' }} />)

    await user.click(view().getByRole('button', { name: /notification options/ }))
    await user.click(await view().findByRole('menuitem', { name: item }))
    await waitFor(() => assert.equal(changes.length, 1))
    assert.equal(shown.mock.callCount(), 0)

    saved.resolve(snapshot())
    await waitFor(() =>
      assert.deepEqual(
        shown.mock.calls.map((call) => call.arguments[0]?.title),
        [toast]
      )
    )
  })

  test(`"${item}" shows the error and no confirmation when the change fails`, async () => {
    const shown = mock.method(notifications, 'show')
    respond = async () => {
      throw new Error('Host unreachable')
    }
    const user = userEvent.setup({ document })
    mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'running' }} />)

    await user.click(view().getByRole('button', { name: /notification options/ }))
    await user.click(await view().findByRole('menuitem', { name: item }))
    await waitFor(() => assert.ok(shown.mock.callCount() > 0))
    await idle()

    assert.deepEqual(
      shown.mock.calls.map((call) => call.arguments[0]?.title),
      ["Couldn't update notification settings"]
    )
  })
}

test('two changes are sent one at a time and the second request carries the first result', async () => {
  const first = Promise.withResolvers<NotificationPreferenceSnapshot>()
  const saved = { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false }
  respond = () => (changes.length === 1 ? first.promise : Promise.resolve(snapshot()))

  const failed = changePreference({ type: 'kind', kind: 'failed', enabled: false })
  const degraded = changePreference({ type: 'kind', kind: 'degraded', enabled: false })
  await waitFor(() => assert.equal(changes.length, 1))
  await idle()
  assert.equal(changes.length, 1)

  first.resolve({ preferences: saved, pipeline_states: {} })

  assert.deepEqual(await Promise.all([failed, degraded]), [true, true])
  assert.deepEqual(changes, [
    { type: 'kind', kind: 'failed', enabled: false },
    { type: 'kind', kind: 'degraded', enabled: false },
  ])
  assert.deepEqual(currents[1], saved)
})

test('a refresh queued behind a change sends the value that change saved', async () => {
  const first = Promise.withResolvers<NotificationPreferenceSnapshot>()
  const saved = { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false }
  respond = async (current) =>
    changes.length === 1
      ? first.promise
      : { preferences: current ?? DEFAULT_NOTIFICATION_PREFERENCES, pipeline_states: {} }

  const change = changePreference({ type: 'kind', kind: 'failed', enabled: false })
  const refresh = refreshPreferenceStates()
  await waitFor(() => assert.equal(changes.length, 1))
  await idle()
  assert.equal(changes.length, 1)

  first.resolve({ preferences: saved, pipeline_states: {} })
  await Promise.all([change, refresh])

  assert.deepEqual(changes[1], { type: 'kind', kind: 'failed', enabled: false })
  assert.equal(useNotificationStore.getState().preferences.failed, false)
})
