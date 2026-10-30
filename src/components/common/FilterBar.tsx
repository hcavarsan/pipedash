import { ActionIcon, Box, Button, Group, MultiSelect, Select, Stack, Tooltip } from '@mantine/core'
import { IconFilter, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

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
  status?: {
    value: string[]
    onSelect: (status: string, selected: boolean) => void
    onClear: () => void
    options?: { value: string; label: string }[]
  }
  provider?: OptionFilter
  organization?: OptionFilter
  repository?: OptionFilter
  workflow?: OptionFilter
  branch?: OptionFilter
  actor?: OptionFilter
  dateRange?: ValueFilter
}

export interface FilterSheet {
  opened: boolean
  onClose: () => void
}

interface FilterBarProps {
  filters: FilterConfig
  onClearAll?: () => void
  sheet?: FilterSheet | undefined
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
  searchable?: boolean | undefined
  leftSection?: React.ReactNode
  clearable?: boolean | undefined
}

export const FilterSelect = ({
  label,
  data,
  value,
  onChange,
  searchable,
  leftSection,
  clearable = true,
}: FilterSelectProps): ReactNode => (
  <Select
    aria-label={label}
    placeholder={label}
    data={data}
    value={value}
    onChange={onChange}
    searchable={searchable ?? false}
    rightSection={
      value && clearable ? (
        <Tooltip label={`Clear ${label.toLowerCase()} filter`}>
          <ActionIcon
            size="xs"
            aria-label={`Clear ${label.toLowerCase()} filter`}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation()
              onChange(null)
            }}
          >
            <IconX size={14} aria-hidden />
          </ActionIcon>
        </Tooltip>
      ) : null
    }
    leftSection={leftSection}
    {...(value && clearable ? { rightSectionPointerEvents: 'all' as const } : {})}
    miw={130}
    flex={1}
    maw="100%"
    size="xs"
  />
)

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
    const clearSheetFilters = () => {
      status?.onClear()
      for (const select of selects) {
        select.onChange(null)
      }
    }

    return (
      <>
        {filters.search && (
          <div className={classes.bar}>
            <PhoneSearchField
              value={searchValue}
              placeholder={filters.search.placeholder}
              onChange={filters.search.onChange}
              onClear={() => filters.search?.onChange('')}
            />
          </div>
        )}
        {activeLabels.length > 0 && (
          <div className={classes.active} role="status">
            <span className={classes.activeText}>Filtered by {activeLabels.join(', ')}</span>
            <button type="button" className={classes.clear} onClick={clearSheetFilters}>
              Clear
            </button>
          </div>
        )}

        <StandardModal opened={sheetOpen} onClose={() => sheet?.onClose()} title="Filters" size="md">
          <Stack gap="lg">
            <IosGroup>
              {status && (
                <IosRow
                  title="Status"
                  accessory={
                    <MultiSelect
                      aria-label="Status"
                      placeholder={status.value.length > 0 ? undefined : 'All'}
                      data={statusOptions}
                      value={status.value}
                      onOptionSubmit={(value) => status.onSelect(value, true)}
                      onRemove={(value) => status.onSelect(value, false)}
                      onClear={status.onClear}
                      variant="unstyled"
                      w={190}
                      styles={{
                        input: { background: 'transparent', paddingLeft: 0 },
                        pillsList: { justifyContent: 'flex-end' },
                      }}
                    />
                  }
                />
              )}
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
                    sheet?.onClose()
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
