import type { FailureKind, ProviderSummary, RefreshResult } from '../types'

const FAILURE_LABELS: Record<FailureKind, string> = {
  network: "can't reach provider",
  timeout: 'timed out',
  tls: 'certificate error',
  rate_limited: 'rate limited',
  unavailable: 'server errors',
  auth: 'token rejected',
  permission: 'missing permissions',
  config: 'invalid configuration',
  not_found: 'not found',
  other: 'failed',
}

const isFailure = (result: RefreshResult): boolean =>
  result.state === 'failed' || (result.state === 'recent' && result.kind !== null)

export function describeRefreshResult(result: RefreshResult): string {
  switch (result.state) {
    case 'ok':
      return 'Refreshed'
    case 'recent':
      return result.kind ? `Not retried yet · last check ${FAILURE_LABELS[result.kind]}` : 'Checked a moment ago'
    case 'pending':
      return 'Still checking'
    case 'not_found':
      return 'Provider no longer exists'
    case 'failed':
      return result.message ?? (result.kind ? FAILURE_LABELS[result.kind] : 'Refresh failed')
  }
}

export interface RefreshSummary {
  message: string
  color: 'green' | 'orange'
}

export function summarizeRefresh(results: RefreshResult[], providers: ProviderSummary[]): RefreshSummary {
  const failed = results.filter(isFailure)
  const pending = results.filter((result) => result.state === 'pending')
  const removed = results.filter((result) => result.state === 'not_found')
  const done = results.length - failed.length - pending.length - removed.length

  if (failed.length === 0 && pending.length === 0 && removed.length === 0) {
    return { message: `Refreshed ${done} ${done === 1 ? 'provider' : 'providers'}`, color: 'green' }
  }

  const parts = [`Refreshed ${done} of ${results.length}`]

  for (const result of failed) {
    const name =
      providers.find((provider) => provider.id === result.provider_id)?.name ?? `Provider ${result.provider_id}`

    parts.push(`${name}: ${result.kind ? FAILURE_LABELS[result.kind] : 'failed'}`)
  }

  if (pending.length > 0) {
    parts.push(`${pending.length} still running`)
  }

  if (removed.length > 0) {
    parts.push(`${removed.length} no longer ${removed.length === 1 ? 'exists' : 'exist'}`)
  }

  return { message: parts.join(' · '), color: 'orange' }
}
