import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { HealthLevel, HealthSignal, Maintenance, ProviderHealth, ProviderSummary } from '../types'

import { formatTime } from './formatDateTime'
import { isOutageLevel, providerAttention, providerIssues, upcomingMaintenance } from './providerAttention'

const signal = (overrides: Partial<HealthSignal> = {}): HealthSignal => ({
  kind: 'status_page',
  episode_id: 'ep-1',
  level: 'major_outage',
  evidence: 'fresh',
  observed_at: '2026-10-22T10:00:00Z',
  title: 'API is down',
  detail: null,
  reported_by: 'GitHub Status',
  url: null,
  components: [],
  incidents: [],
  maintenances: [],
  updated_at: '2026-10-22T10:00:00Z',
  ...overrides,
})

const health = (level: HealthLevel, signals: HealthSignal[] = [], overrides: Partial<ProviderHealth> = {}) => ({
  level,
  signals,
  next_attempt_at: null,
  in_flight: false,
  status_page: null,
  ...overrides,
})

const summary = (overrides: Partial<ProviderSummary> = {}): ProviderSummary => ({
  id: 7,
  name: 'GitHub',
  provider_type: 'github',
  icon: null,
  pipeline_count: 3,
  last_updated: null,
  refresh_interval: 30,
  configured_repositories: [],
  last_fetch_status: 'success',
  last_fetch_error: null,
  last_fetch_error_kind: null,
  last_fetch_at: null,
  paused_until: null,
  health: health('operational'),
  ...overrides,
})

const maintenance = (overrides: Partial<Maintenance> = {}): Maintenance => ({
  id: 'm1',
  name: 'Database upgrade',
  in_progress: false,
  starts_at: null,
  ends_at: null,
  url: null,
  ...overrides,
})

test('operational provider needs no attention', () => {
  const result = providerAttention(summary())

  assert.equal(result.tone, null)
  assert.equal(result.headline, null)
  assert.equal(result.hint, null)
  assert.equal(result.episodeKey, null)
  assert.equal(result.refreshingNormally, false)
})

test('partial outage or worse is a warning headed by the lead signal title', () => {
  for (const level of ['partial_outage', 'major_outage'] as const) {
    const result = providerAttention(summary({ health: health(level, [signal({ level })]) }))

    assert.equal(result.tone, 'warning')
    assert.equal(result.headline, 'API is down')
    assert.equal(result.episodeKey, '7:ep-1')
  }
})

test('an outage outranks a non-transient fetch error', () => {
  const result = providerAttention(
    summary({
      last_fetch_status: 'error',
      last_fetch_error: 'bad token',
      last_fetch_error_kind: 'auth',
      health: health('major_outage', [signal()]),
    })
  )

  assert.equal(result.tone, 'warning')
  assert.equal(result.hint, 'The token was rejected. Update it in Settings → Providers.')
})

test('non-transient fetch errors are danger and outrank degraded', () => {
  for (const kind of ['auth', 'permission', 'config', 'tls', 'other', 'not_found'] as const) {
    const result = providerAttention(
      summary({
        last_fetch_status: 'error',
        last_fetch_error: 'boom',
        last_fetch_error_kind: kind,
        health: health('degraded', [signal({ level: 'degraded', title: 'Slow' })]),
      })
    )

    assert.equal(result.tone, 'danger', kind)
    assert.equal(result.headline, 'Fetch failed')
    assert.equal(result.detail, 'boom')
    assert.equal(result.kind, kind)
    assert.equal(result.episodeKey, `7:fetch-failed:${kind}`)
  }
})

test('transient fetch errors alone do not raise attention', () => {
  for (const kind of ['network', 'timeout', 'rate_limited', 'unavailable'] as const) {
    const result = providerAttention(
      summary({ last_fetch_status: 'error', last_fetch_error: 'x', last_fetch_error_kind: kind })
    )

    assert.equal(result.tone, null, kind)
  }
})

test('degraded is a warning and maintenance in progress is info', () => {
  const degraded = providerAttention(summary({ health: health('degraded', [signal({ level: 'degraded' })]) }))
  const inProgress = providerAttention(
    summary({
      health: health('maintenance', [
        signal({
          level: 'maintenance',
          title: 'Scheduled maintenance',
          maintenances: [maintenance({ in_progress: true })],
        }),
      ]),
    })
  )

  assert.equal(degraded.tone, 'warning')
  assert.equal(inProgress.tone, 'info')
  assert.equal(inProgress.headline, 'Scheduled maintenance')
})

test('upcoming maintenance at operational level does not raise attention', () => {
  const result = providerAttention(
    summary({
      health: health('operational', [
        signal({ level: 'operational', maintenances: [maintenance({ starts_at: '2099-01-01T00:00:00Z' })] }),
      ]),
    })
  )

  assert.equal(result.tone, null)
})

test('stale signals do not set the tone and are described as unreachable', () => {
  const result = providerAttention(
    summary({ health: health('operational', [signal({ evidence: 'stale', title: 'API is down' })]) })
  )

  assert.equal(result.tone, null)
  assert.deepEqual(result.staleNotes, [
    `Status page unreachable since ${formatTime('2026-10-22T10:00:00Z')}. Last report: API is down.`,
  ])
})

test('hints follow the failure kind and fall back to the raw message', () => {
  const hintFor = (kind: ProviderSummary['last_fetch_error_kind'], message = 'raw message') =>
    providerAttention(
      summary({
        last_fetch_status: 'error',
        last_fetch_error: message,
        last_fetch_error_kind: kind,
        health: health('major_outage', [signal()]),
      })
    ).hint

  assert.equal(hintFor('auth'), 'The token was rejected. Update it in Settings → Providers.')
  assert.equal(hintFor('permission'), 'The token is missing permissions. Check them in Settings → Providers.')
  assert.equal(hintFor('config'), 'Check the provider URL and certificate settings.')
  assert.equal(hintFor('tls'), 'Check the provider URL and certificate settings.')
  assert.equal(
    hintFor('network'),
    "Pipedash can't reach this provider. Check the network or VPN of the machine running Pipedash."
  )
  assert.equal(hintFor('timeout'), hintFor('network'))
  assert.equal(hintFor('rate_limited'), 'Rate limited.')
  assert.equal(hintFor('unavailable'), 'The provider is returning server errors.')
  assert.equal(hintFor('other'), 'raw message')
  assert.equal(hintFor(null, 'no kind'), 'no kind')
})

test('next attempt and checking now are appended to the hint', () => {
  const failing = (extra: Partial<ProviderHealth>) =>
    providerAttention(
      summary({
        last_fetch_status: 'error',
        last_fetch_error: 'down',
        last_fetch_error_kind: 'network',
        health: health('major_outage', [signal({ kind: 'connection' })], extra),
      })
    ).hint

  const next = '2026-10-22T10:05:00Z'

  assert.equal(failing({ next_attempt_at: next }), `Next attempt after ${formatTime(next)}`)
  assert.equal(failing({ in_flight: true, next_attempt_at: next }), 'Checking now')
})

test('a provider outage without a fetch error has only the schedule as hint', () => {
  const quiet = providerAttention(summary({ health: health('major_outage', [signal()]) }))
  const checking = providerAttention(summary({ health: health('major_outage', [signal()], { in_flight: true }) }))

  assert.equal(quiet.hint, null)
  assert.equal(checking.hint, 'Checking now')
})

test('refreshing normally needs a successful fetch, no connection signal and no backoff', () => {
  const normal = summary({ health: health('major_outage', [signal()]) })

  assert.equal(providerAttention(normal).refreshingNormally, true)
  assert.equal(providerAttention({ ...normal, last_fetch_status: 'never' }).refreshingNormally, false)
  assert.equal(providerAttention({ ...normal, last_fetch_status: 'error' }).refreshingNormally, false)
  assert.equal(
    providerAttention({
      ...normal,
      health: health('major_outage', [signal(), signal({ kind: 'connection', episode_id: 'c1' })]),
    }).refreshingNormally,
    false
  )
  assert.equal(
    providerAttention({
      ...normal,
      health: health('major_outage', [signal()], { next_attempt_at: '2026-10-22T10:05:00Z' }),
    }).refreshingNormally,
    false
  )
})

test('upcoming maintenance picks the earliest future window of fresh signals', () => {
  const now = Date.parse('2026-10-22T12:00:00Z')
  const target = summary({
    health: health('operational', [
      signal({
        level: 'operational',
        maintenances: [
          maintenance({ id: 'past', starts_at: '2026-10-21T00:00:00Z' }),
          maintenance({ id: 'running', in_progress: true, starts_at: '2026-10-22T11:00:00Z' }),
          maintenance({ id: 'later', starts_at: '2026-10-25T00:00:00Z' }),
          maintenance({ id: 'soon', starts_at: '2026-10-23T00:00:00Z' }),
          maintenance({ id: 'unscheduled' }),
        ],
      }),
      signal({
        evidence: 'stale',
        maintenances: [maintenance({ id: 'stale', starts_at: '2026-10-22T13:00:00Z' })],
      }),
    ]),
  })

  assert.equal(upcomingMaintenance(target, now)?.id, 'soon')
  assert.equal(upcomingMaintenance(summary(), now), null)
})

test('outage levels start at partial outage', () => {
  assert.equal(isOutageLevel('degraded'), false)
  assert.equal(isOutageLevel('partial_outage'), true)
  assert.equal(isOutageLevel('major_outage'), true)
})

test('issues list only providers that need attention, worst tone first', () => {
  const issues = providerIssues([
    summary({ id: 1, name: 'Healthy' }),
    summary({
      id: 2,
      name: 'Maintenance',
      health: health('maintenance', [
        signal({ level: 'maintenance', maintenances: [maintenance({ in_progress: true })] }),
      ]),
    }),
    summary({ id: 3, name: 'Outage', health: health('major_outage', [signal()]) }),
    summary({ id: 4, name: 'Broken token', last_fetch_status: 'error', last_fetch_error_kind: 'auth' }),
  ])

  assert.deepEqual(
    issues.map((issue) => [issue.provider.name, issue.tone]),
    [
      ['Broken token', 'danger'],
      ['Outage', 'warning'],
      ['Maintenance', 'info'],
    ]
  )
})
