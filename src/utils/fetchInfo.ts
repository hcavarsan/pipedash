import type { ProviderSummary } from '../types'

import { formatDateTime } from './formatDateTime'
import { formatRelativeTime } from './formatRelativeTime'

export interface FetchInfoLines {
  name: string
  lastFetch: string
  status: string
}

const statusText = (provider: ProviderSummary) => {
  if (provider.last_fetch_status === 'error') {
    return provider.last_fetch_error ? `Failed: ${provider.last_fetch_error}` : 'Failed'
  }

  return provider.last_fetch_status === 'success' ? 'Success' : 'Not fetched yet'
}

export const describeFetch = (provider: ProviderSummary, now: number): FetchInfoLines => ({
  name: provider.name,
  lastFetch: provider.last_fetch_at
    ? `${formatRelativeTime(provider.last_fetch_at, now)} (${formatDateTime(provider.last_fetch_at)})`
    : 'Never',
  status: statusText(provider),
})

export type ProviderTone = 'fresh' | 'stale' | 'down' | 'never'

export const TONE_META: Record<ProviderTone, { label: string; color: string }> = {
  fresh: { label: 'Up to date', color: 'var(--status-success)' },
  stale: { label: 'Stale', color: 'var(--status-pending)' },
  down: { label: 'Down', color: 'var(--status-failed)' },
  never: { label: 'Never fetched', color: 'var(--status-neutral)' },
}

const TONE_SEVERITY: ProviderTone[] = ['down', 'stale', 'never', 'fresh']

export const providerTone = (provider: ProviderSummary, now: number): ProviderTone => {
  const { state, stale_at: staleAt } = provider.freshness

  return state === 'fresh' && staleAt !== null && now > Date.parse(staleAt) ? 'stale' : state
}

export const worstTone = (tones: ProviderTone[]): ProviderTone =>
  TONE_SEVERITY.find((tone) => tones.includes(tone)) ?? 'fresh'

export const providerBadge = (provider: ProviderSummary, now: number): { color: string; label: string } | undefined => {
  const tone = providerTone(provider, now)

  return tone === 'fresh' ? undefined : { color: TONE_META[tone].color, label: TONE_META[tone].label }
}

interface DataAge {
  tone: ProviderTone
  label: string
  detail: string
}

export const dataAge = (provider: ProviderSummary, now: number): DataAge => {
  const tone = providerTone(provider, now)
  const at = provider.last_success_at
  const ago = at ? formatRelativeTime(at, now) : null

  return {
    tone,
    label: !ago
      ? TONE_META[tone].label
      : tone === 'fresh'
        ? `Data checked ${ago}`
        : `${TONE_META[tone].label} · data checked ${ago}`,
    detail: at
      ? `Pipeline data from ${formatDateTime(at)}`
      : tone === 'never'
        ? 'No pipeline data fetched yet'
        : 'Last successful fetch time unknown',
  }
}

export const staleNote = (provider: ProviderSummary, now: number): string | undefined => {
  const age = dataAge(provider, now)

  return age.tone === 'fresh' ? undefined : age.label
}
