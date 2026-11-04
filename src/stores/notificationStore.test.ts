import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()
after(() => GlobalRegistrator.unregister())

const {
  DEFAULT_NOTIFICATION_PREFERENCES,
  migrateNotificationSettings,
  notificationAllowed,
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

test('a notification passes only when its kind is on and its source is not muted', () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    recovered: false,
    muted_providers: [2],
    muted_pipelines: ['github__1__api'],
  }

  assert.equal(notificationAllowed(preferences, 'failed', [1], 'github__1__web'), true)
  assert.equal(notificationAllowed(preferences, 'failed', [1], 'github__1__api'), false)
  assert.equal(notificationAllowed(preferences, 'recovered', [1], null), false)
  assert.equal(notificationAllowed(preferences, 'outage', [2], null), false)
  assert.equal(notificationAllowed(preferences, 'outage', [2, 3], null), true)
  assert.equal(notificationAllowed(preferences, 'outage', [], null), true)
  assert.equal(notificationAllowed(preferences, 'test', [2], 'github__1__api'), true)
})

const PIPELINE = 'github__1__acme/api__ci.yml'

test('a finished run only notifies for a watched pipeline', () => {
  const watched = { ...DEFAULT_NOTIFICATION_PREFERENCES, watched_pipelines: [PIPELINE] }

  assert.equal(notificationAllowed(DEFAULT_NOTIFICATION_PREFERENCES, 'finished', [1], PIPELINE), false)
  assert.equal(notificationAllowed(watched, 'finished', [1], PIPELINE), true)
  assert.equal(notificationAllowed(watched, 'finished', [1], 'github__1__acme/api__other.yml'), false)
  assert.equal(notificationAllowed(watched, 'finished', [1], null), false)
})

test('watching beats muted providers and switched-off alert types', () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    failed: false,
    degraded: false,
    muted_providers: [1],
    watched_pipelines: [PIPELINE],
  }

  assert.equal(notificationAllowed(preferences, 'failed', [1], PIPELINE), true)
  assert.equal(notificationAllowed(preferences, 'degraded', [1], PIPELINE), true)
  assert.equal(notificationAllowed(preferences, 'finished', [1], PIPELINE), true)
  assert.equal(notificationAllowed(preferences, 'failed', [1], 'github__1__acme/api__other.yml'), false)
})

test('a muted pipeline blocks everything but tests, even when it is also watched', () => {
  const since = '2026-11-01T10:00:00Z'
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    muted_pipelines: [PIPELINE],
    watched_pipelines: [PIPELINE],
    watch_once: [{ pipeline_id: PIPELINE, since }],
  }

  assert.equal(notificationAllowed(preferences, 'failed', [1], PIPELINE), false)
  assert.equal(
    notificationAllowed(preferences, 'finished', [1], PIPELINE, { at: '2026-11-01T11:00:00Z', previous: null }),
    false
  )
  assert.equal(notificationAllowed(preferences, 'test', [1], PIPELINE), true)
})

test('a one-time request matches only the first finish after it was made', () => {
  const since = '2026-11-01T10:00:00Z'
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    failed: false,
    muted_providers: [1],
    watch_once: [{ pipeline_id: PIPELINE, since }],
  }
  const allowed = (finish: { at: string; previous: string | null } | null, id: string | null = PIPELINE) =>
    notificationAllowed(preferences, 'finished', [1], id, finish)

  assert.equal(allowed({ at: '2026-11-01T09:59:59Z', previous: null }), false)
  assert.equal(allowed({ at: '2026-11-01T09:00:00Z', previous: '2026-11-01T08:00:00Z' }), false)
  assert.equal(allowed({ at: '2026-11-01T10:30:00Z', previous: null }), true)
  assert.equal(allowed({ at: '2026-11-01T10:00:00Z', previous: '2026-11-01T09:00:00Z' }), true)
  assert.equal(allowed({ at: '2026-11-01T11:00:00Z', previous: '2026-11-01T10:30:00Z' }), false)
  assert.equal(allowed({ at: '2026-11-01T11:00:00Z', previous: '2026-11-01T10:00:00Z' }), false)
  assert.equal(allowed({ at: '2026-11-01T10:30:00Z', previous: null }, 'github__1__acme/api__other.yml'), false)
  assert.equal(allowed(null), false)
  assert.equal(
    notificationAllowed(preferences, 'failed', [1], PIPELINE, { at: '2026-11-01T10:30:00Z', previous: null }),
    true
  )
  assert.equal(notificationAllowed(preferences, 'failed', [1], PIPELINE), false)
})

test('one-time requests compare instants, not strings', () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    watch_once: [{ pipeline_id: PIPELINE, since: '2026-11-01T10:00:00+02:00' }],
  }

  assert.equal(
    notificationAllowed(preferences, 'finished', [1], PIPELINE, { at: '2026-11-01T08:30:00Z', previous: null }),
    true
  )
  assert.equal(
    notificationAllowed(preferences, 'finished', [1], PIPELINE, { at: '2026-11-01T09:30:00+02:00', previous: null }),
    false
  )
})

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

test('asking again replaces the pending request, and cancelling or consuming drops it', () => {
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

  store.consumeNextFinish('other')
  assert.deepEqual(lists().watch_once, [])
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
  assert.equal(notificationAllowed(preferences, 'finished', [1], 'github__1__web'), false)
})
