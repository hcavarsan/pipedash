import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactElement } from 'react'

import { pipelineFixture, providerSummaryFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: { invoke: async () => null },
})

const { cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { queryKeys } = await import('../../../lib/queryKeys')
const { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } = await import('../../../stores/notificationStore')
const { NotificationPreferencesSection } = await import('./NotificationPreferencesSection')
const { PipelineNotifyMenu } = await import('./PipelineNotifyMenu')

const view = () => within(document.body)
const pipeline = pipelineFixture({ id: 'github__1__acme/api__ci.yml', name: 'API CI', repository: 'acme/api' })
const preferences = () => useNotificationStore.getState().preferences

function mount(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  client.setQueryData(queryKeys.pipelines.list(), [pipeline])
  client.setQueryData(queryKeys.providers.list(), [providerSummaryFixture({ id: 1, name: 'GitHub' })])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>{ui}</QueryClientProvider>
    </MantineProvider>
  )
}

afterEach(() => {
  cleanup()
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES })
})
after(() => GlobalRegistrator.unregister())

test('the pipeline menu button names the current state', () => {
  const states = [
    { patch: {}, label: 'Notifications' },
    { patch: { watched_pipelines: [pipeline.id] }, label: 'Watching' },
    { patch: { watch_once: [{ pipeline_id: pipeline.id, since: '2026-11-01T10:00:00Z' }] }, label: 'Notify on finish' },
    { patch: { muted_pipelines: [pipeline.id] }, label: 'Muted' },
  ]

  for (const { patch, label } of states) {
    useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, ...patch } })
    mount(<PipelineNotifyMenu pipeline={pipeline} />)
    assert.ok(view().getByRole('button', { name: `${label}, notification options` }))
    cleanup()
  }
})

test('the pipeline menu offers the finish option that fits the run state and switches the pipeline to watching', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'running' }} />)

  await user.click(view().getByRole('button', { name: /notification options/ }))
  assert.ok(await view().findByRole('menuitem', { name: 'Notify when this run finishes' }))
  assert.ok(view().getByRole('menuitem', { name: 'Mute notifications' }))

  await user.click(view().getByRole('menuitem', { name: 'Watch all runs' }))
  assert.deepEqual(preferences().watched_pipelines, [pipeline.id])

  await user.click(view().getByRole('button', { name: /notification options/ }))
  assert.ok(await view().findByRole('menuitem', { name: 'Stop watching' }))
  assert.equal(view().queryByRole('menuitem', { name: /finishes$/ }), null)
})

test('an idle pipeline offers the next-run option, a pending one can be cancelled, and muting replaces both', async () => {
  const user = userEvent.setup({ document })
  mount(<PipelineNotifyMenu pipeline={{ ...pipeline, status: 'success' }} />)

  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Notify when the next run finishes' }))
  assert.deepEqual(
    preferences().watch_once.map((watch) => watch.pipeline_id),
    [pipeline.id]
  )

  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Cancel finish notification' }))
  assert.deepEqual(preferences().watch_once, [])

  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Notify when the next run finishes' }))
  await user.click(view().getByRole('button', { name: /notification options/ }))
  await user.click(await view().findByRole('menuitem', { name: 'Mute notifications' }))
  assert.deepEqual(preferences().muted_pipelines, [pipeline.id])
  assert.deepEqual(preferences().watch_once, [])
})

test('settings list watched pipelines and pending requests, and each can be undone', async () => {
  const user = userEvent.setup({ document })
  const other = 'github__1__acme/web__deploy.yml'
  useNotificationStore.setState({
    preferences: {
      ...DEFAULT_NOTIFICATION_PREFERENCES,
      watched_pipelines: [pipeline.id],
      watch_once: [{ pipeline_id: other, since: '2026-11-01T10:00:00Z' }],
    },
  })
  mount(<NotificationPreferencesSection />)

  assert.ok(view().getByText('API CI'))
  assert.ok(view().getByText(/Next finish only/))

  await user.click(view().getByRole('button', { name: 'Stop watching API CI' }))
  assert.deepEqual(preferences().watched_pipelines, [])

  await user.click(view().getByRole('button', { name: `Cancel finish notification for ${other}` }))
  assert.deepEqual(preferences().watch_once, [])
  assert.ok(view().getByText(/choose Watch all runs/))
})
