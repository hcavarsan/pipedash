import type { PipelineView, PipelineViewQuery } from '../types'

import { rootViewOf } from './viewTree'

const PROVIDER_TAB_PREFIX = 'provider:'

export const providerTabId = (providerId: number): string => `${PROVIDER_TAB_PREFIX}${providerId}`

export const activeTabId = (views: PipelineView[], query: PipelineViewQuery): string =>
  query.view === 'all' && query.provider_id !== undefined
    ? providerTabId(query.provider_id)
    : (rootViewOf(views, query.view)?.id ?? query.view)

export const tabSearch = (current: URLSearchParams, id: string): URLSearchParams => {
  const next = new URLSearchParams(current)

  next.delete('provider')

  if (id.startsWith(PROVIDER_TAB_PREFIX)) {
    next.delete('view')
    next.set('provider', id.slice(PROVIDER_TAB_PREFIX.length))
  } else if (id === 'all') {
    next.delete('view')
  } else {
    next.set('view', id)
  }

  return next
}
