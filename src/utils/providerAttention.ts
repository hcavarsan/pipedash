import type { FailureKind, HealthLevel, HealthSignal, ProviderSummary } from '../types'

import { formatTime } from './formatDateTime'

export type AttentionTone = 'warning' | 'danger'

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

export const fetchHint = (summary: ProviderSummary): string | null => {
  if (summary.last_fetch_status !== 'error') {
    return null
  }

  const kind = summary.last_fetch_error_kind

  return (kind && HINTS[kind]) ?? summary.last_fetch_error
}

export const scheduleNote = ({ health }: ProviderSummary): string | null => {
  if (health.in_flight) {
    return 'Checking now'
  }

  return health.next_attempt_at ? `Next attempt after ${formatTime(health.next_attempt_at)}` : null
}

const staleNote = (signal: HealthSignal): string => {
  const source = signal.kind === 'status_page' ? 'Status page' : signal.reported_by

  return `${source} unreachable since ${formatTime(signal.observed_at)}. Last report: ${signal.title}.`
}

interface Lead {
  tone: AttentionTone
  headline: string
  detail: string | null
  episodeKey: string
  explainsFailure: boolean
}

function pickLead(summary: ProviderSummary, fresh: HealthSignal[], kind: FailureKind | null): Lead | null {
  const [first] = fresh

  if (first && isOutageLevel(summary.health.level)) {
    return {
      tone: 'warning',
      headline: first.title,
      detail: first.detail,
      episodeKey: `${summary.id}:${first.episode_id}`,
      explainsFailure: first.kind === 'connection',
    }
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

  return null
}

export function providerAttention(summary: ProviderSummary): ProviderAttention {
  const { health } = summary
  const kind = summary.last_fetch_status === 'error' ? summary.last_fetch_error_kind : null
  const fresh = health.signals.filter((signal) => signal.evidence === 'fresh')
  const staleNotes = health.signals.filter((signal) => signal.evidence === 'stale').map(staleNote)
  const lead = pickLead(summary, fresh, kind)
  const refreshingNormally =
    summary.last_fetch_status === 'success' &&
    !health.signals.some((signal) => signal.kind === 'connection') &&
    health.next_attempt_at === null

  if (!lead) {
    return {
      tone: null,
      headline: null,
      detail: null,
      hint: null,
      kind,
      episodeKey: null,
      refreshingNormally,
      staleNotes,
    }
  }

  const base = lead.explainsFailure ? null : fetchHint(summary)

  return {
    tone: lead.tone,
    headline: lead.headline,
    detail: lead.detail,
    hint: [base, scheduleNote(summary)].filter(Boolean).join(' ') || null,
    kind,
    episodeKey: lead.episodeKey,
    refreshingNormally,
    staleNotes,
  }
}
