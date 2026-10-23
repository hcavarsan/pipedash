import type { FailureKind, HealthLevel, HealthSignal, Maintenance, ProviderSummary } from '../types'

import { formatTime } from './formatDateTime'

export type AttentionTone = 'warning' | 'danger' | 'info'

export interface ProviderAttention {
  tone: AttentionTone | null
  headline: string | null
  detail: string | null
  hint: string | null
  kind: FailureKind | null
  episodeKey: string | null
  refreshingNormally: boolean
  staleNotes: string[]
}

const LEVEL_RANK: Record<HealthLevel, number> = {
  operational: 0,
  maintenance: 1,
  degraded: 2,
  partial_outage: 3,
  major_outage: 4,
}

const TRANSIENT_KINDS: Partial<Record<FailureKind, true>> = {
  network: true,
  timeout: true,
  rate_limited: true,
  unavailable: true,
}

const UNREACHABLE_HINT = "Pipedash can't reach this provider. Check the network or VPN of the machine running Pipedash."
const SETTINGS_HINT = 'Check the provider URL and certificate settings.'

const HINTS: Partial<Record<FailureKind, string>> = {
  auth: 'The token was rejected. Update it in Settings → Providers.',
  permission: 'The token is missing permissions. Check them in Settings → Providers.',
  config: SETTINGS_HINT,
  tls: SETTINGS_HINT,
  network: UNREACHABLE_HINT,
  timeout: UNREACHABLE_HINT,
  rate_limited: 'Rate limited.',
  unavailable: 'The provider is returning server errors.',
}

export const isOutageLevel = (level: HealthLevel): boolean => LEVEL_RANK[level] >= LEVEL_RANK.partial_outage

const staleNote = (signal: HealthSignal): string => {
  const source = signal.kind === 'status_page' ? 'Status page' : signal.reported_by

  return `${source} unreachable since ${formatTime(signal.observed_at)}. Last report: ${signal.title}.`
}

interface Lead {
  tone: NonNullable<ProviderAttention['tone']>
  headline: string
  detail: string | null
  episodeKey: string
  explainsFailure: boolean
}

const fromSignal = (summary: ProviderSummary, tone: Lead['tone'], signal: HealthSignal): Lead => ({
  tone,
  headline: signal.title,
  detail: signal.detail,
  episodeKey: `${summary.id}:${signal.episode_id}`,
  explainsFailure: signal.kind === 'connection' || signal.kind === 'rate_limit',
})

function pickLead(summary: ProviderSummary, fresh: HealthSignal[], kind: FailureKind | null): Lead | null {
  const { level } = summary.health
  const first = fresh[0]

  if (first && isOutageLevel(level)) {
    return fromSignal(summary, 'warning', first)
  }

  if (summary.last_fetch_status === 'error' && (kind === null || !TRANSIENT_KINDS[kind])) {
    return {
      tone: 'danger',
      headline: 'Fetch failed',
      detail: summary.last_fetch_error,
      episodeKey: `${summary.id}:fetch-failed:${kind ?? 'other'}`,
      explainsFailure: false,
    }
  }

  if (first && level === 'degraded') {
    return fromSignal(summary, 'warning', first)
  }

  const maintenance = fresh.find(
    (signal) => signal.level === 'maintenance' || signal.maintenances.some((item) => item.in_progress)
  )

  return maintenance ? fromSignal(summary, 'info', maintenance) : null
}

export function providerAttention(summary: ProviderSummary): ProviderAttention {
  const { health } = summary
  const failed = summary.last_fetch_status === 'error'
  const kind = failed ? summary.last_fetch_error_kind : null
  const fresh = health.signals.filter((signal) => signal.evidence === 'fresh')
  const staleNotes = health.signals.filter((signal) => signal.evidence === 'stale').map(staleNote)
  const lead = pickLead(summary, fresh, kind)

  if (!lead) {
    return {
      tone: null,
      headline: null,
      detail: null,
      hint: null,
      kind,
      episodeKey: null,
      refreshingNormally: false,
      staleNotes,
    }
  }

  const schedule = health.in_flight
    ? 'Checking now'
    : health.next_attempt_at
      ? `Next attempt after ${formatTime(health.next_attempt_at)}`
      : null
  const base = failed && !lead.explainsFailure ? ((kind && HINTS[kind]) ?? summary.last_fetch_error) : null
  const hint = [base, schedule].filter(Boolean).join(' ') || null

  return {
    tone: lead.tone,
    headline: lead.headline,
    detail: lead.detail,
    hint,
    kind,
    episodeKey: lead.episodeKey,
    refreshingNormally:
      summary.last_fetch_status === 'success' &&
      !health.signals.some((signal) => signal.kind === 'connection') &&
      health.next_attempt_at === null,
    staleNotes,
  }
}

export function upcomingMaintenance(summary: ProviderSummary, now: number): Maintenance | null {
  let next: Maintenance | null = null
  let nextStart = Number.POSITIVE_INFINITY

  for (const signal of summary.health.signals) {
    if (signal.evidence !== 'fresh') {
      continue
    }

    for (const item of signal.maintenances) {
      const start = item.starts_at ? Date.parse(item.starts_at) : Number.NaN

      if (!item.in_progress && start > now && start < nextStart) {
        next = item
        nextStart = start
      }
    }
  }

  return next
}
