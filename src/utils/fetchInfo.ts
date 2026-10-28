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
const MIN_STALE_MS = 10 * 60_000

export const providerTone = (provider: ProviderSummary, now: number): ProviderTone => {
  const { health, last_fetch_status: status, last_fetch_at: fetchedAt, paused_until: pausedUntil } = provider

  if (status === 'error' || health.level === 'partial_outage' || health.level === 'major_outage') {
    return 'down'
  }

  if (status === 'never') {
    return 'never'
  }

  const staleAfter = Math.max(3 * provider.refresh_interval * 1000, MIN_STALE_MS)
  const paused = pausedUntil !== null && Date.parse(pausedUntil) > now
  const old = fetchedAt !== null && now - Date.parse(fetchedAt) > staleAfter

  return paused || old ? 'stale' : 'fresh'
}

export const worstTone = (tones: ProviderTone[]): ProviderTone =>
  TONE_SEVERITY.find((tone) => tones.includes(tone)) ?? 'fresh'

export const providerBadge = (provider: ProviderSummary, now: number) => {
  const tone = providerTone(provider, now)

  return tone === 'fresh' ? undefined : { color: TONE_META[tone].color, label: TONE_META[tone].label }
}
