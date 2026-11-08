import type { ReactNode } from 'react'
import { FilterBar } from '../../../components/common/FilterBar'
import type { FilterSheet } from '../../../components/common/filterTypes'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'

interface PipelineViewFiltersProps {
  model: PipelineViewPageModel
  sheet: FilterSheet
}

export function PipelineViewFilters({ model, sheet }: PipelineViewFiltersProps): ReactNode {
  const { page, filters, setFilter, selectStatus, clearFilters, providerOptions } = model

  if (!page) {
    return null
  }

  return (
    <FilterBar
      sheet={sheet}
      filters={{
        search: {
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: page.presentation.copy.search_placeholder,
        },
        status: {
          value: filters.status,
          onSelect: selectStatus,
          onClear: () => setFilter('status', []),
          options: page.presentation.status_options,
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
          label: page.presentation.terminology.group_title,
          allLabel: `All ${page.presentation.terminology.groups}`,
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
