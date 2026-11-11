import type { ProviderFreshness, ProviderSummary, ProviderTone } from '../types/providers'

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

export const TONE_META: Record<ProviderTone, { label: string; color: string }> = {
  fresh: { label: 'Up to date', color: 'var(--status-success)' },
  stale: { label: 'Stale', color: 'var(--status-pending)' },
  down: { label: 'Down', color: 'var(--status-failed)' },
  never: { label: 'Never fetched', color: 'var(--status-neutral)' },
}

export const freshnessTone = ({ state, stale_at: staleAt }: ProviderFreshness, now: number): ProviderTone =>
  state === 'fresh' && staleAt !== null && now > Date.parse(staleAt) ? 'stale' : state

export const providerTone = (provider: ProviderSummary, now: number): ProviderTone =>
  freshnessTone(provider.freshness, now)

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

  const missing = tone === 'never' ? 'No pipeline data fetched yet' : 'Last successful fetch time unknown'
  const checked = tone === 'fresh' ? `Data checked ${ago}` : `${TONE_META[tone].label} · data checked ${ago}`

  return {
    tone,
    label: ago ? checked : TONE_META[tone].label,
    detail: at ? `Pipeline data from ${formatDateTime(at)}` : missing,
  }
}
