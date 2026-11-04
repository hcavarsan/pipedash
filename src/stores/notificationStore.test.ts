import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()
after(() => GlobalRegistrator.unregister())

const {
  DEFAULT_NOTIFICATION_PREFERENCES,
  migrateNotificationSettings,
  pendingWatches,
  remapPreferences,
  useNotificationStore,
} = await import('./notificationStore')

const enabled = (notificationsEnabled: boolean) => ({
  notificationsEnabled,
  notifyTriggeredRuns: true,
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
  savedLists: {},
})

test('either legacy toggle keeps notifications on', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: true, providerAlertsEnabled: false }, 0),
    enabled(true)
  )
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: true }, 0),
    enabled(true)
  )
})

test('both legacy toggles off, or nothing saved, stays off', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: false }, 0),
    enabled(false)
  )
  assert.deepEqual(migrateNotificationSettings(undefined, 0), enabled(false))
})

test('the unified flag is kept once saved and ignores stray legacy keys', () => {
  assert.deepEqual(migrateNotificationSettings({ notificationsEnabled: true }, 1), enabled(true))
  assert.deepEqual(
    migrateNotificationSettings({ notificationsEnabled: false, providerAlertsEnabled: true }, 1),
    enabled(false)
  )
})

test('saved preferences keep what was chosen and drop malformed entries', () => {
  const since = '2026-11-01T10:00:00Z'
  const { preferences } = migrateNotificationSettings(
    {
      notificationsEnabled: true,
      preferences: {
        long_run: false,
        outage: 'no',
        muted_providers: [3, '4', 1.5],
        muted_pipelines: ['a', 7],
        watched_pipelines: ['w', null],
        watch_once: [
          { pipeline_id: 'o', since, extra: 1 },
          { pipeline_id: 'bad-date', since: 'yesterday' },
          { pipeline_id: 5, since },
          { pipeline_id: 'no-since' },
          'o',
        ],
      },
    },
    2
  )

  assert.deepEqual(preferences, {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    long_run: false,
    muted_providers: [3],
    muted_pipelines: ['a'],
    watched_pipelines: ['w'],
    watch_once: [{ pipeline_id: 'o', since }],
  })
})

const PIPELINE = 'github__1__acme/api__ci.yml'

const lists = () => useNotificationStore.getState().preferences

test('watching and muting a pipeline replace each other, and a pending request goes with either', () => {
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES })
  const store = useNotificationStore.getState()

  store.setPipelineMuted(PIPELINE, true)
  store.watchNextFinish(PIPELINE)
  assert.deepEqual(lists().muted_pipelines, [])
  assert.equal(lists().watch_once.length, 1)

  store.setPipelineMuted(PIPELINE, true)
  assert.deepEqual(lists().muted_pipelines, [PIPELINE])
  assert.deepEqual(lists().watch_once, [])

  store.setPipelineWatched(PIPELINE, true)
  assert.deepEqual(lists().muted_pipelines, [])
  assert.deepEqual(lists().watched_pipelines, [PIPELINE])

  store.setPipelineMuted(PIPELINE, true)
  assert.deepEqual(lists().watched_pipelines, [])
  assert.deepEqual(lists().muted_pipelines, [PIPELINE])

  store.setPipelineMuted(PIPELINE, false)
  assert.deepEqual(lists().muted_pipelines, [])
})

test('watching clears a pending one-time request', () => {
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES })
  const store = useNotificationStore.getState()

  store.watchNextFinish(PIPELINE)
  store.setPipelineWatched(PIPELINE, true)
  assert.deepEqual(lists().watch_once, [])
  assert.deepEqual(lists().watched_pipelines, [PIPELINE])

  store.setPipelineWatched(PIPELINE, false)
  assert.deepEqual(lists().watched_pipelines, [])
})

test('asking again replaces the pending request, and cancelling drops it', () => {
  useNotificationStore.setState({
    preferences: {
      ...DEFAULT_NOTIFICATION_PREFERENCES,
      watch_once: [
        { pipeline_id: PIPELINE, since: '2026-01-01T00:00:00Z' },
        { pipeline_id: 'other', since: '2026-01-01T00:00:00Z' },
      ],
    },
  })
  const store = useNotificationStore.getState()

  store.watchNextFinish(PIPELINE)
  const mine = lists().watch_once.filter((watch) => watch.pipeline_id === PIPELINE)
  assert.equal(mine.length, 1)
  assert.ok(Date.parse(mine[0]?.since ?? '') > Date.parse('2026-01-01T00:00:00Z'))
  assert.equal(lists().watch_once.length, 2)

  store.cancelNextFinish(PIPELINE)
  assert.deepEqual(
    lists().watch_once.map((watch) => watch.pipeline_id),
    ['other']
  )

  store.cancelNextFinish('other')
  assert.deepEqual(lists().watch_once, [])
})

const WATCH = { pipeline_id: PIPELINE, since: '2026-11-01T10:00:00.000Z' }
const FIRED_AT = '2026-11-01T10:30:00.000Z'

test('marking a request fired keeps it but it stops being pending', () => {
  const other = { pipeline_id: 'other', since: WATCH.since }
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [WATCH, other] } })

  useNotificationStore.getState().markFired([{ ...WATCH, since: '2026-11-01T10:00:00Z', fired_at: FIRED_AT }])

  assert.deepEqual(lists().watch_once, [{ ...WATCH, fired_at: FIRED_AT }, other])
  assert.deepEqual(pendingWatches(lists()), [other])
})

test('a fired mark needs the same pipeline and request time, and the first mark stays', () => {
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [WATCH] } })
  const store = useNotificationStore.getState()

  store.markFired([
    { pipeline_id: PIPELINE, since: '2026-11-01T10:00:01.000Z', fired_at: FIRED_AT },
    { pipeline_id: 'other', since: WATCH.since, fired_at: FIRED_AT },
  ])
  assert.deepEqual(lists().watch_once, [WATCH])

  store.markFired([{ ...WATCH, fired_at: FIRED_AT }])
  const marked = lists()
  store.markFired([{ ...WATCH, fired_at: '2026-11-02T00:00:00.000Z' }])
  assert.equal(lists(), marked)
  assert.equal(lists().watch_once[0]?.fired_at, FIRED_AT)
})

test('asking again after a request fired makes a new pending one', () => {
  useNotificationStore.setState({
    preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [{ ...WATCH, fired_at: FIRED_AT }] },
  })

  useNotificationStore.getState().watchNextFinish(PIPELINE)

  assert.equal(pendingWatches(lists()).length, 1)
  assert.equal(lists().watch_once.length, 1)
})

test('a valid fired mark survives reading saved preferences and a malformed one does not', () => {
  const { preferences } = migrateNotificationSettings(
    {
      preferences: {
        watch_once: [
          { ...WATCH, fired_at: FIRED_AT },
          { pipeline_id: 'bad', since: WATCH.since, fired_at: 'later' },
          { pipeline_id: 'null', since: WATCH.since, fired_at: null },
        ],
      },
    },
    2
  )

  assert.deepEqual(preferences.watch_once, [
    { ...WATCH, fired_at: FIRED_AT },
    { pipeline_id: 'bad', since: WATCH.since },
    { pipeline_id: 'null', since: WATCH.since },
  ])
})

test('adopting server preferences replaces the current ones and validates them', () => {
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, muted_providers: [9] } })

  useNotificationStore.getState().adoptPreferences({
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    failed: false,
    watch_once: [{ ...WATCH, fired_at: FIRED_AT }],
  })

  assert.deepEqual(lists(), {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    failed: false,
    watch_once: [{ ...WATCH, fired_at: FIRED_AT }],
  })
})

test('a storage migration moves mutes and watches to the new provider IDs and drops removed providers', () => {
  const since = '2026-11-01T10:00:00Z'
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    muted_providers: [2, 5],
    muted_pipelines: ['github__2__acme/api__ci.yml', 'gitlab__5__group/app', 'broken'],
    watched_pipelines: ['github__2__acme/web__ci.yml', 'gitlab__5__group/app', 'broken'],
    watch_once: [
      { pipeline_id: 'github__3__acme/api__ci.yml', since },
      { pipeline_id: 'gitlab__5__group/app', since },
      { pipeline_id: 'broken', since },
    ],
  }

  assert.deepEqual(remapPreferences(preferences, { 2: 1, 3: 2 }), {
    ...preferences,
    muted_providers: [1],
    muted_pipelines: ['github__1__acme/api__ci.yml'],
    watched_pipelines: ['github__1__acme/web__ci.yml'],
    watch_once: [{ pipeline_id: 'github__2__acme/api__ci.yml', since }],
  })
})

const deferredRemap = () => {
  const preferences = { ...DEFAULT_NOTIFICATION_PREFERENCES, muted_providers: [2, 5] }
  useNotificationStore.setState({ preferences })
  useNotificationStore.getState().remapProvidersOnRestart({ 2: 1, 3: 2 })
  assert.deepEqual(useNotificationStore.getState().preferences, preferences)
}
const mutedProviders = () => useNotificationStore.getState().preferences.muted_providers

test('a local-core migration keeps the old mutes until the next launch, then remaps them once', async () => {
  deferredRemap()

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])
})

test('a phone remaps its mutes as soon as the core reloads on the new database', async () => {
  Object.assign(window, { __TAURI_INTERNALS__: { invoke: async () => undefined } })
  const { tauriService } = await import('../services/tauri')
  deferredRemap()

  await tauriService.restartApp()
  assert.deepEqual(mutedProviders(), [1])

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])
})

test('mutes and watches stay with the server they were set on and come back when it is chosen again', () => {
  const chosen = {
    muted_providers: [1],
    muted_pipelines: ['github__1__api'],
    watched_pipelines: ['github__1__web'],
    watch_once: [{ pipeline_id: 'github__1__db', since: '2026-11-01T10:00:00Z' }],
  }
  useNotificationStore.setState({
    source: 'client:https://a.example',
    savedLists: {},
    preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, long_run: false, ...chosen },
  })
  const { selectSource } = useNotificationStore.getState()

  selectSource('client:https://b.example')
  assert.deepEqual(useNotificationStore.getState().preferences, {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    long_run: false,
  })

  selectSource('client:https://a.example')
  assert.deepEqual(useNotificationStore.getState().preferences, {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    long_run: false,
    ...chosen,
  })
})

test('launching drops one-time requests older than a week and keeps newer ones', async () => {
  const daysAgo = (days: number) => new Date(Date.now() - days * 24 * 60 * 60 * 1000).toISOString()
  useNotificationStore.setState({
    source: 'client:https://a.example',
    preferences: {
      ...DEFAULT_NOTIFICATION_PREFERENCES,
      watch_once: [
        { pipeline_id: 'stale', since: daysAgo(8) },
        { pipeline_id: 'recent', since: daysAgo(6) },
      ],
    },
    savedLists: {
      'client:https://b.example': {
        muted_providers: [],
        muted_pipelines: [],
        watched_pipelines: ['kept'],
        watch_once: [{ pipeline_id: 'stale-elsewhere', since: daysAgo(9) }],
      },
    },
  })

  await useNotificationStore.persist.rehydrate()

  const { preferences, savedLists } = useNotificationStore.getState()
  assert.deepEqual(
    preferences.watch_once.map((watch) => watch.pipeline_id),
    ['recent']
  )
  assert.deepEqual(savedLists['client:https://b.example']?.watch_once, [])
  assert.deepEqual(savedLists['client:https://b.example']?.watched_pipelines, ['kept'])
})

test('settings saved before watching existed still load with empty watch lists', async () => {
  localStorage.setItem(
    'pipedash-notifications',
    JSON.stringify({
      state: { notificationsEnabled: true, preferences: { outage: false, muted_pipelines: ['github__1__api'] } },
      version: 2,
    })
  )

  await useNotificationStore.persist.rehydrate()

  const { preferences } = useNotificationStore.getState()
  assert.deepEqual(preferences, {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    outage: false,
    muted_pipelines: ['github__1__api'],
  })
})
