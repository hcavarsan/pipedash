import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { HostAlert } from '../types/domain'

GlobalRegistrator.register({ width: 1200, height: 800 })

const callbacks = new Map<number, (event: unknown) => void>()
const listeners = new Map<string, number>()
const calls: string[] = []
let nextCallback = 0
let batches: HostAlert[][] = []
let failure: Error | null = null
let gate: Promise<void> | null = null

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
      calls.push(command)
      switch (command) {
        case 'get_connection':
          return { mode: 'client', host: 'Studio', status: 'connected' }
        case 'plugin:event|listen':
          listeners.set(String(args.event), Number(args.handler))

          return args.handler
        case 'take_missed_alerts':
          if (gate) {
            await gate
          }
          if (failure) {
            throw failure
          }

          return batches.shift() ?? []
        default:
          return undefined
      }
    },
  },
})

const { act, cleanup, render } = await import('@testing-library/react')
const { createElement } = await import('react')
const { loadConnection } = await import('../services/connection')
const { useNotificationCenterStore } = await import('../stores/notificationCenterStore')
const { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } = await import('../stores/notificationStore')
const { useMissedAlerts } = await import('./useMissedAlerts')

await loadConnection()

const failed: HostAlert = {
  id: '00000000000000000000000000000001',
  seq: 1,
  key: 'pipeline:p1',
  kind: 'failed',
  tone: 'danger',
  title: 'Build failed',
  body: 'owner/repo · github',
  pipeline_id: 'p1',
  provider_id: 1,
  created_at: '2026-11-03T12:00:00Z',
}
const recovered: HostAlert = {
  id: '00000000000000000000000000000002',
  seq: 2,
  key: 'status-page:github',
  kind: 'recovered',
  tone: 'success',
  title: 'GitHub Actions recovered',
  body: 'Workflow runs are back to normal.',
  pipeline_id: null,
  provider_id: null,
  created_at: '2026-11-03T12:05:00Z',
}

function Probe() {
  useMissedAlerts()

  return null
}

const taken = () => calls.filter((command) => command === 'take_missed_alerts').length

async function emitStatus(status: string) {
  const id = listeners.get('connection-status')
  assert.ok(id)
  await act(async () => {
    callbacks.get(id)?.({ event: 'connection-status', id: 1, payload: { status, reconnectAttempts: 0 } })
  })
}

beforeEach(() => {
  calls.length = 0
  batches = []
  failure = null
  gate = null
})

afterEach(async () => {
  await act(async () => cleanup())
  useNotificationCenterStore.setState({ items: [], selection: null })
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES })
})

after(() => GlobalRegistrator.unregister())

test('alerts missed while away are pulled on mount, even if the connection came up before React listened', async () => {
  batches = [[failed, recovered]]

  await act(async () => {
    render(createElement(Probe))
  })

  assert.equal(taken(), 1)
  const items = useNotificationCenterStore.getState().items
  assert.deepEqual(
    items.map(({ key, tone, providerIds, pipelineId }) => ({ key, tone, providerIds, pipelineId })),
    [
      { key: 'status-page:github', tone: 'success', providerIds: [], pipelineId: null },
      { key: 'pipeline:p1', tone: 'danger', providerIds: [1], pipelineId: 'p1' },
    ]
  )
  assert.equal(items.at(-1)?.at, Date.parse(failed.created_at))
})

test('alerts pulled while the view unmounts still reach the center', async () => {
  batches = [[failed]]
  const { promise, resolve } = Promise.withResolvers<void>()
  gate = promise
  let unmount: () => void = () => undefined

  await act(async () => {
    unmount = render(createElement(Probe)).unmount
  })
  await act(async () => unmount())
  await act(async () => resolve())

  assert.deepEqual(
    useNotificationCenterStore.getState().items.map(({ key }) => key),
    ['pipeline:p1']
  )
})

test('each reconnect pulls again and other statuses do not', async () => {
  await act(async () => {
    render(createElement(Probe))
  })
  assert.equal(taken(), 1)

  await emitStatus('reconnecting')
  await emitStatus('connecting')
  assert.equal(taken(), 1)

  batches = [[failed]]
  await emitStatus('connected')
  assert.equal(taken(), 2)
  assert.deepEqual(
    useNotificationCenterStore.getState().items.map(({ key }) => key),
    ['pipeline:p1']
  )
})

test('a failed pull is logged and the next connection pulls again', async () => {
  failure = new Error('host unreachable')
  await act(async () => {
    render(createElement(Probe))
  })
  assert.equal(useNotificationCenterStore.getState().items.length, 0)

  failure = null
  batches = [[recovered]]
  await emitStatus('connected')

  assert.deepEqual(
    useNotificationCenterStore.getState().items.map(({ key }) => key),
    ['status-page:github']
  )
})

test('every alert the host sends is pushed, with no rule applied and no one-time request consumed', async () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    failed: false,
    recovered: false,
    muted_providers: [1],
    muted_pipelines: ['p1', 'p2'],
    watch_once: [{ pipeline_id: 'p2', since: '2026-11-03T11:00:00.000Z' }],
  }
  useNotificationStore.setState({ preferences })
  const finished: HostAlert = {
    ...failed,
    id: '00000000000000000000000000000003',
    seq: 3,
    key: 'pipeline:p2',
    kind: 'finished',
    tone: 'success',
    pipeline_id: 'p2',
    finish: { at: '2026-11-03T12:10:00Z', previous: null },
    created_at: '2026-11-03T12:10:00Z',
  }
  batches = [[failed, recovered, finished]]

  await act(async () => {
    render(createElement(Probe))
  })

  assert.deepEqual(
    useNotificationCenterStore.getState().items.map(({ key }) => key),
    ['pipeline:p2', 'status-page:github', 'pipeline:p1']
  )
  assert.equal(useNotificationStore.getState().preferences, preferences)
})
