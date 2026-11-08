import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactElement } from 'react'

import { pipelineFixture, providerSummaryFixture } from '../../../testing/fixtures'
import type { NotificationPreferenceSnapshot, PreferenceChange } from '../../../types/notification_preferences'

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

const view = () => within(document.body)
const pipeline = pipelineFixture({ id: 'stub__1__acme/api__ci.yml', name: 'API CI', repository: 'acme/api' })
const changes: PreferenceChange[] = []
let reply: NotificationPreferenceSnapshot

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

beforeEach(() => {
  changes.length = 0
  reply = snapshot()
  const change = async (next: PreferenceChange) => {
    changes.push(next)

    return reply
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

test('the pipeline menu offers the finish option that fits the run state and sends the chosen change', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'running' }} />)

  await user.click(view().getByRole('button', { name: /notification options/ }))
  assert.ok(await view().findByRole('menuitem', { name: 'Notify when this run finishes' }))
  assert.ok(view().getByRole('menuitem', { name: 'Mute notifications' }))

  reply = snapshot({ [pipeline.id]: 'watching' })
  await user.click(view().getByRole('menuitem', { name: 'Watch all runs' }))
  await waitFor(() =>
    assert.deepEqual(changes, [{ type: 'pipeline_watched', pipeline_id: pipeline.id, watched: true }])
  )

  await user.click(await view().findByRole('button', { name: 'Watching, notification options' }))
  assert.ok(await view().findByRole('menuitem', { name: 'Stop watching' }))
  assert.equal(view().queryByRole('menuitem', { name: /finishes$/ }), null)
})

test('an idle pipeline offers the next-run option and a pending one can be cancelled', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'success' }} />)

  reply = snapshot({ [pipeline.id]: 'pending' })
  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Notify when the next run finishes' }))
  await waitFor(() => assert.deepEqual(changes.at(-1), { type: 'watch_next_finish', pipeline_id: pipeline.id }))

  reply = snapshot()
  await user.click(await view().findByRole('button', { name: 'Notify on finish, notification options' }))
  await user.click(await view().findByRole('menuitem', { name: 'Cancel finish notification' }))
  await waitFor(() => assert.deepEqual(changes.at(-1), { type: 'cancel_next_finish', pipeline_id: pipeline.id }))

  await user.click(await view().findByRole('button', { name: 'Notifications, notification options' }))
  await user.click(await view().findByRole('menuitem', { name: 'Mute notifications' }))
  await waitFor(() =>
    assert.deepEqual(changes.at(-1), { type: 'pipeline_muted', pipeline_id: pipeline.id, muted: true })
  )
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
