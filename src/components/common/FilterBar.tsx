import { ActionIcon, Box, Button, Group, Select, TextInput } from '@mantine/core'
import { IconFilter, IconSearch, IconX } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { DEBOUNCE_DELAYS } from '../../constants/intervals'
import { useDebounce } from '../../hooks/useDebounce'
import { STATUS_OPTIONS } from '../../theme/status'
import { DATE_RANGE_OPTIONS } from '../../utils/dateRange'

type FilterOption = string | { value: string; label: string }

interface ValueFilter {
  value: string | null
  onChange: (value: string | null) => void
}

interface OptionFilter extends ValueFilter {
  options: FilterOption[]
}

interface FilterConfig {
  search?: {
    value: string
    onChange: (value: string) => void
    placeholder?: string
  }
  status?: ValueFilter
  provider?: OptionFilter
  organization?: OptionFilter
  repository?: OptionFilter
  workflow?: OptionFilter
  branch?: OptionFilter
  actor?: OptionFilter
  dateRange?: ValueFilter
}

interface FilterBarProps {
  filters: FilterConfig
  onClearAll?: () => void
}

const statusOptions = [{ value: '', label: 'All statuses' }, ...STATUS_OPTIONS]

const dateRangeOptions = [{ value: '', label: 'All time' }, ...DATE_RANGE_OPTIONS]

type OptionFilterKey = 'provider' | 'organization' | 'repository' | 'workflow' | 'branch' | 'actor'

const OPTION_FILTERS: { key: OptionFilterKey; label: string; allLabel: string }[] = [
  { key: 'provider', label: 'Provider', allLabel: 'All providers' },
  { key: 'organization', label: 'Organization', allLabel: 'All organizations' },
  { key: 'repository', label: 'Repository', allLabel: 'All repositories' },
  { key: 'workflow', label: 'Workflow', allLabel: 'All workflows' },
  { key: 'branch', label: 'Branch', allLabel: 'All branches' },
  { key: 'actor', label: 'Actor', allLabel: 'All actors' },
]

interface FilterSelectProps {
  label: string
  data: { value: string; label: string }[]
  value: string | null
  onChange: (value: string | null) => void
  searchable?: boolean
  leftSection?: React.ReactNode
}

const FilterSelect = ({ label, data, value, onChange, searchable, leftSection }: FilterSelectProps) => (
  <Select
    aria-label={label}
    placeholder={label}
    data={data}
    value={value}
    onChange={onChange}
    clearable
    searchable={searchable}
    clearButtonProps={{ 'aria-label': `Clear ${label.toLowerCase()} filter` }}
    leftSection={leftSection}
    miw={130}
    flex={1}
    maw="100%"
    size="xs"
  />
)

export const FilterBar = ({ filters, onClearAll }: FilterBarProps) => {
  const [localSearchValue, setLocalSearchValue] = useState(filters.search?.value || '')
  const debouncedSearchValue = useDebounce(localSearchValue, DEBOUNCE_DELAYS.FILTER)

  const syncStateRef = useRef<{
    lastSentToParent: string
    lastReceivedFromParent: string
  }>({
    lastSentToParent: filters.search?.value || '',
    lastReceivedFromParent: filters.search?.value || '',
  })

  useEffect(() => {
    const searchFilter = filters.search

    if (!searchFilter) {
      return
    }

    const externalValue = searchFilter.value ?? ''
    const syncState = syncStateRef.current

    if (externalValue !== syncState.lastReceivedFromParent) {
      syncState.lastReceivedFromParent = externalValue

      if (externalValue !== localSearchValue && externalValue !== debouncedSearchValue) {
        setLocalSearchValue(externalValue)
        syncState.lastSentToParent = externalValue
      }

      return
    }

    if (debouncedSearchValue !== syncState.lastSentToParent) {
      syncState.lastSentToParent = debouncedSearchValue
      syncState.lastReceivedFromParent = debouncedSearchValue
      searchFilter.onChange(debouncedSearchValue)
    }
  }, [debouncedSearchValue, filters.search, localSearchValue])

  const hasActiveFilters = useMemo(
    () =>
      !!(
        filters.search?.value ||
        filters.status?.value ||
        filters.provider?.value ||
        filters.organization?.value ||
        filters.repository?.value ||
        filters.workflow?.value ||
        filters.branch?.value ||
        filters.actor?.value ||
        filters.dateRange?.value
      ),
    [
      filters.search,
      filters.status,
      filters.provider,
      filters.organization,
      filters.repository,
      filters.workflow,
      filters.branch,
      filters.actor,
      filters.dateRange,
    ]
  )

  const clearAllFilters = useCallback(() => {
    setLocalSearchValue('')
    if (onClearAll) {
      onClearAll()

      return
    }
    // Fallback: clear individually
    filters.search?.onChange('')
    filters.status?.onChange(null)
    filters.provider?.onChange(null)
    filters.organization?.onChange(null)
    filters.repository?.onChange(null)
    filters.workflow?.onChange(null)
    filters.branch?.onChange(null)
    filters.actor?.onChange(null)
    filters.dateRange?.onChange(null)
  }, [filters, onClearAll])

  return (
    <Box mb="md">
      <Group gap="sm" wrap="wrap" align="flex-start">
        {filters.search && (
          <TextInput
            aria-label={filters.search.placeholder || 'Search'}
            placeholder={filters.search.placeholder || 'Search'}
            leftSection={<IconSearch size={14} />}
            rightSection={
              localSearchValue && (
                <ActionIcon
                  size="xs"
                  variant="subtle"
                  color="gray"
                  aria-label="Clear search"
                  onClick={() => setLocalSearchValue('')}
                >
                  <IconX size={14} />
                </ActionIcon>
              )
            }
            value={localSearchValue}
            onChange={(e) => setLocalSearchValue(e.currentTarget.value)}
            miw={200}
            flex={1}
            maw="100%"
            size="xs"
          />
        )}

        {filters.status && (
          <FilterSelect
            label="Status"
            data={statusOptions}
            value={filters.status.value}
            onChange={filters.status.onChange}
            leftSection={<IconFilter size={14} />}
          />
        )}

        {OPTION_FILTERS.map(({ key, label, allLabel }) => {
          const filter = filters[key]

          if (!filter || filter.options.length === 0) {
            return null
          }

          return (
            <FilterSelect
              key={key}
              label={label}
              data={[
                { value: '', label: allLabel },
                ...filter.options.map((option) =>
                  typeof option === 'string' ? { value: option, label: option } : option
                ),
              ]}
              value={filter.value}
              onChange={filter.onChange}
              searchable
            />
          )
        })}

        {filters.dateRange && (
          <FilterSelect
            label="Time range"
            data={dateRangeOptions}
            value={filters.dateRange.value}
            onChange={filters.dateRange.onChange}
          />
        )}

        {hasActiveFilters && (
          <Button variant="subtle" color="gray" onClick={clearAllFilters} size="xs" leftSection={<IconX size={14} />}>
            Clear filters
          </Button>
        )}
      </Group>
    </Box>
  )
}
