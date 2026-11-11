import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()
after(() => GlobalRegistrator.unregister())

const { DEFAULT_NOTIFICATION_PREFERENCES, migrateNotificationSettings, useNotificationStore } = await import(
  './notificationStore'
)

const enabled = (notificationsEnabled: boolean) => ({
  notificationsEnabled,
  notifyTriggeredRuns: true,
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
  pipelineStates: {},
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

const PIPELINE = 'stub__1__acme/api__ci.yml'
const lists = () => useNotificationStore.getState().preferences

const WATCH = { pipeline_id: PIPELINE, since: '2026-11-01T10:00:00.000Z' }
const FIRED_AT = '2026-11-01T10:30:00.000Z'

test('marking a request fired keeps it but it stops being pending', () => {
  const other = { pipeline_id: 'other', since: WATCH.since }
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [WATCH, other] } })

  useNotificationStore.getState().markFired([{ ...WATCH, since: '2026-11-01T10:00:00Z', fired_at: FIRED_AT }])

  assert.deepEqual(lists().watch_once, [{ ...WATCH, fired_at: FIRED_AT }, other])
  assert.deepEqual(
    lists().watch_once.filter((watch) => !watch.fired_at),
    [other]
  )
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

test('a backend snapshot replaces the preferences and the pipeline states together', () => {
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES, pipelineStates: {} })

  useNotificationStore.getState().applySnapshot({
    preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, muted_pipelines: [PIPELINE] },
    pipeline_states: { [PIPELINE]: 'muted' },
  })

  assert.deepEqual(lists().muted_pipelines, [PIPELINE])
  assert.deepEqual(useNotificationStore.getState().pipelineStates, { [PIPELINE]: 'muted' })
})
