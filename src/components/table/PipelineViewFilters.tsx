import type { ReactNode } from 'react'

import { getStatusOptions } from '../../utils/pipelinePresentation'
import { FilterBar } from '../common/FilterBar'

import type { PipelineViewPageModel } from './usePipelineViewModel'

export function PipelineViewFilters({ model }: { model: PipelineViewPageModel }): ReactNode {
  const { page, filters, setFilter, selectStatus, clearFilters, isFluxView, labels, providerOptions } = model

  if (!page) {
    return null
  }

  return (
    <FilterBar
      filters={{
        search: {
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: isFluxView ? 'Search resources, namespaces, sources…' : 'Search repositories, workflows…',
        },
        status: {
          value: filters.status,
          onSelect: selectStatus,
          onClear: () => setFilter('status', []),
          options: getStatusOptions(isFluxView ? 'fluxcd' : undefined),
        },
        provider: {
          value: filters.provider,
          onChange: (value) => setFilter('provider', value),
          options: providerOptions,
        },
        organization: {
          value: filters.organization,
          onChange: (value) => setFilter('organization', value),
          options: page.facets.organizations,
          label: labels.groupTitle,
          allLabel: `All ${labels.groups}`,
        },
        dateRange: {
          value: filters.dateRange,
          onChange: (value) => setFilter('dateRange', value),
        },
      }}
      onClearAll={clearFilters}
    />
  )
}
