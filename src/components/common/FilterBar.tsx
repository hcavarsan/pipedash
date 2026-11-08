import { Box, Button, Group, MultiSelect } from '@mantine/core'
import { IconFilter, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { STATUS_OPTIONS } from '../../theme/status'
import { DATE_RANGE_OPTIONS } from '../../utils/dateRange'

import classes from './FilterBar.module.css'
import { FilterSelect } from './FilterSelect'
import { FilterSheet } from './FilterSheet'
import type { FilterBarProps } from './filterTypes'
import { PhoneSearchField } from './PhoneSearchField'
import { SearchInput } from './SearchInput'

type OptionFilterKey = 'provider' | 'organization' | 'repository' | 'workflow' | 'branch' | 'actor'

const OPTION_FILTERS: { key: OptionFilterKey; label: string; allLabel: string }[] = [
  { key: 'provider', label: 'Provider', allLabel: 'All providers' },
  { key: 'organization', label: 'Organization', allLabel: 'All organizations' },
  { key: 'repository', label: 'Repository', allLabel: 'All repositories' },
  { key: 'workflow', label: 'Workflow', allLabel: 'All workflows' },
  { key: 'branch', label: 'Branch', allLabel: 'All branches' },
  { key: 'actor', label: 'Actor', allLabel: 'All actors' },
]

const dateRangeOptions = [{ value: '', label: 'All time' }, ...DATE_RANGE_OPTIONS]

export const FilterBar = ({ filters, onClearAll, sheet }: FilterBarProps): ReactNode => {
  const mobileShell = useMobileShell()
  const sheetOpen = sheet?.opened ?? false
  const searchValue = filters.search?.value ?? ''

  const hasActiveFilters = !!(
    filters.search?.value ||
    filters.status?.value.length ||
    filters.provider?.value ||
    filters.organization?.value ||
    filters.repository?.value ||
    filters.workflow?.value ||
    filters.branch?.value ||
    filters.actor?.value ||
    filters.dateRange?.value
  )

  const clearAllFilters = (): void => {
    if (onClearAll) {
      onClearAll()

      return
    }
    filters.search?.onChange('')
    filters.status?.onClear()
    filters.provider?.onChange(null)
    filters.organization?.onChange(null)
    filters.repository?.onChange(null)
    filters.workflow?.onChange(null)
    filters.branch?.onChange(null)
    filters.actor?.onChange(null)
    filters.dateRange?.onChange(null)
  }

  const status = filters.status
  const statusOptions = status?.options ?? STATUS_OPTIONS

  const selects = [
    ...OPTION_FILTERS.flatMap(({ key, label, allLabel }) => {
      const filter = filters[key]

      if (!filter || filter.options.length === 0) {
        return []
      }

      return [
        {
          key,
          label: filter.label ?? label,
          data: [
            { value: '', label: filter.allLabel ?? allLabel },
            ...filter.options.map((option) => (typeof option === 'string' ? { value: option, label: option } : option)),
          ],
          value: filter.value,
          onChange: filter.onChange,
          searchable: true,
        },
      ]
    }),
    ...(filters.dateRange
      ? [
          {
            key: 'dateRange',
            label: 'Time range',
            data: dateRangeOptions,
            value: filters.dateRange.value,
            onChange: filters.dateRange.onChange,
            searchable: false,
          },
        ]
      : []),
  ]

  const searchInput = () =>
    filters.search && (
      <Box miw={200} flex={1} maw="100%">
        <SearchInput
          aria-label={filters.search.placeholder || 'Search'}
          placeholder={filters.search.placeholder || 'Search'}
          value={searchValue}
          onChange={filters.search.onChange}
          size="xs"
        />
      </Box>
    )

  if (mobileShell) {
    const activeLabels = [
      ...(status?.value.length ? ['Status'] : []),
      ...selects.filter((select) => select.value).map((select) => select.label),
    ]
    const clearSheetFilters = (): void => {
      status?.onClear()
      for (const select of selects) {
        select.onChange(null)
      }
    }
    const searchBar = filters.search && (
      <div className={classes.bar}>
        <PhoneSearchField
          value={searchValue}
          placeholder={filters.search.placeholder}
          onChange={filters.search.onChange}
          onClear={() => filters.search?.onChange('')}
        />
      </div>
    )
    const activeBar = activeLabels.length > 0 && (
      <div className={classes.active} role="status">
        <span className={classes.activeText}>Filtered by {activeLabels.join(', ')}</span>
        <button type="button" className={classes.clear} onClick={clearSheetFilters}>
          Clear
        </button>
      </div>
    )

    return (
      <>
        <div
          className={classes.reveal}
          data-search-reveal
          data-pinned={searchValue || activeLabels.length > 0 || sheetOpen ? true : undefined}
        >
          {searchBar}
          {activeBar}
        </div>

        <FilterSheet
          opened={sheetOpen}
          onClose={() => sheet?.onClose()}
          status={status}
          statusOptions={statusOptions}
          selects={selects}
          hasActiveFilters={hasActiveFilters}
          onClear={clearAllFilters}
        />
      </>
    )
  }

  return (
    <Box>
      <Group gap="sm" wrap="wrap" align="flex-start">
        {searchInput()}

        {status && (
          <MultiSelect
            aria-label="Status"
            placeholder={status.value.length > 0 ? undefined : 'Status'}
            data={statusOptions}
            value={status.value}
            onOptionSubmit={(value) => status.onSelect(value, true)}
            onRemove={(value) => status.onSelect(value, false)}
            onClear={status.onClear}
            clearable
            clearButtonProps={{ 'aria-label': 'Clear status filter' }}
            leftSection={<IconFilter size={14} aria-hidden />}
            miw={130}
            flex={1}
            maw="100%"
            size="xs"
          />
        )}

        {selects.map(({ key, label, data, value, onChange, searchable }) => (
          <FilterSelect key={key} label={label} data={data} value={value} onChange={onChange} searchable={searchable} />
        ))}

        {hasActiveFilters && (
          <Button
            variant="subtle"
            color="gray"
            onClick={clearAllFilters}
            size="xs"
            leftSection={<IconX size={14} aria-hidden />}
          >
            Clear filters
          </Button>
        )}
      </Group>
    </Box>
  )
}
