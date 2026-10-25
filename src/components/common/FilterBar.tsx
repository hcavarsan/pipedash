import { ActionIcon, Box, Button, Group, Select, Stack, Tooltip } from '@mantine/core'
import { IconAdjustmentsHorizontal, IconFilter, IconFilterFilled, IconX } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { DEBOUNCE_DELAYS } from '../../constants/intervals'
import { useDebounce } from '../../hooks/useDebounce'
import { useMobileShell } from '../../hooks/useIsMobile'
import { STATUS_OPTIONS } from '../../theme/status'
import { DATE_RANGE_OPTIONS } from '../../utils/dateRange'
import { IosGroup, IosRow } from '../ios/IosList'

import classes from './FilterBar.module.css'

import { PhoneSearchField } from './PhoneSearchField'
import { SearchInput } from './SearchInput'
import { StandardModal } from './StandardModal'

type FilterOption = string | { value: string; label: string }

interface ValueFilter {
  value: string | null
  onChange: (value: string | null) => void
}

interface OptionFilter extends ValueFilter {
  options: FilterOption[]
  label?: string
  allLabel?: string
}

interface FilterConfig {
  search?: {
    value: string
    onChange: (value: string) => void
    placeholder?: string
  }
  status?: ValueFilter & { options?: { value: string; label: string }[] }
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
  clearable?: boolean
}

export const FilterSelect = ({
  label,
  data,
  value,
  onChange,
  searchable,
  leftSection,
  clearable = true,
}: FilterSelectProps) => (
  <Select
    aria-label={label}
    placeholder={label}
    data={data}
    value={value}
    onChange={onChange}
    searchable={searchable}
    rightSection={
      value && clearable ? (
        <Tooltip label={`Clear ${label.toLowerCase()} filter`}>
          <ActionIcon
            variant="subtle"
            color="gray"
            size="xs"
            aria-label={`Clear ${label.toLowerCase()} filter`}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation()
              onChange(null)
            }}
          >
            <IconX size={14} />
          </ActionIcon>
        </Tooltip>
      ) : undefined
    }
    rightSectionPointerEvents={value && clearable ? 'all' : undefined}
    leftSection={leftSection}
    miw={130}
    flex={1}
    maw="100%"
    size="xs"
  />
)

export const FilterBar = ({ filters, onClearAll }: FilterBarProps) => {
  const mobileShell = useMobileShell()
  const [sheetOpen, setSheetOpen] = useState(false)
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

  const selects = [
    ...(filters.status
      ? [
          {
            key: 'status',
            label: 'Status',
            data: [{ value: '', label: 'All statuses' }, ...(filters.status.options ?? STATUS_OPTIONS)],
            value: filters.status.value,
            onChange: filters.status.onChange,
            leftSection: <IconFilter size={14} />,
            searchable: false,
          },
        ]
      : []),
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
          leftSection: undefined,
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
            leftSection: undefined,
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
          value={localSearchValue}
          onChange={setLocalSearchValue}
          size="xs"
        />
      </Box>
    )

  if (mobileShell) {
    const activeSelects = selects.filter((select) => select.value).length

    return (
      <>
        <div
          className={classes.reveal}
          data-search-reveal
          data-pinned={localSearchValue || activeSelects > 0 || sheetOpen ? true : undefined}
        >
          <div className={classes.bar}>
            {filters.search && (
              <PhoneSearchField
                value={localSearchValue}
                placeholder={filters.search.placeholder}
                onChange={setLocalSearchValue}
                onClear={() => {
                  setLocalSearchValue('')
                  filters.search?.onChange('')
                }}
              />
            )}
            {selects.length > 0 && (
              <button
                type="button"
                className={classes.filter}
                data-active={activeSelects > 0 || undefined}
                aria-label={activeSelects > 0 ? `Filters, ${activeSelects} active` : 'Filters'}
                onClick={() => setSheetOpen(true)}
              >
                {activeSelects > 0 ? (
                  <IconFilterFilled size={22} aria-hidden />
                ) : (
                  <IconAdjustmentsHorizontal size={22} aria-hidden />
                )}
              </button>
            )}
          </div>
        </div>

        <StandardModal opened={sheetOpen} onClose={() => setSheetOpen(false)} title="Filters" size="md">
          <Stack gap="lg">
            <IosGroup>
              {selects.map(({ key, label, data, value, onChange, searchable }) => (
                <IosRow
                  key={key}
                  title={label}
                  accessory={
                    <Select
                      aria-label={label}
                      placeholder="All"
                      data={data}
                      value={value}
                      onChange={onChange}
                      searchable={searchable}
                      variant="unstyled"
                      w={190}
                      styles={{ input: { background: 'transparent', textAlign: 'right', paddingLeft: 0 } }}
                    />
                  }
                />
              ))}
            </IosGroup>
            {hasActiveFilters && (
              <IosGroup>
                <IosRow
                  title="Clear filters"
                  destructive
                  onClick={() => {
                    clearAllFilters()
                    setSheetOpen(false)
                  }}
                />
              </IosGroup>
            )}
          </Stack>
        </StandardModal>
      </>
    )
  }

  return (
    <Box>
      <Group gap="sm" wrap="wrap" align="flex-start">
        {searchInput()}

        {selects.map(({ key, label, data, value, onChange, searchable, leftSection }) => (
          <FilterSelect
            key={key}
            label={label}
            data={data}
            value={value}
            onChange={onChange}
            searchable={searchable}
            leftSection={leftSection}
          />
        ))}

        {hasActiveFilters && (
          <Button variant="subtle" color="gray" onClick={clearAllFilters} size="xs" leftSection={<IconX size={14} />}>
            Clear filters
          </Button>
        )}
      </Group>
    </Box>
  )
}
