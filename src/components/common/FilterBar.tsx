import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { ActionIcon, Box, Button, Group, Select, TextInput } from '@mantine/core'
import { IconFilter, IconSearch, IconX } from '@tabler/icons-react'

import { DEBOUNCE_DELAYS } from '../../constants/intervals'
import { useDebounce } from '../../hooks/useDebounce'

interface FilterConfig {
  search?: {
    value: string;
    onChange: (value: string) => void;
    placeholder?: string;
  };
  status?: {
    value: string | null;
    onChange: (value: string | null) => void;
  };
  provider?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  organization?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  repository?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  workflow?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  branch?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  actor?: {
    value: string | null;
    onChange: (value: string | null) => void;
    options: string[];
  };
  dateRange?: {
    value: string | null;
    onChange: (value: string | null) => void;
  };
}

interface FilterBarProps {
  filters: FilterConfig;
  onClearAll?: () => void;
}

const statusOptions = [
  { value: '', label: 'All Statuses' },
  { value: 'success', label: '✓ Success' },
  { value: 'failed', label: '✗ Failed' },
  { value: 'running', label: '⟳ Running' },
  { value: 'pending', label: '⋯ Pending' },
  { value: 'cancelled', label: '⊘ Cancelled' },
  { value: 'skipped', label: '⊗ Skipped' },
]

const dateRangeOptions = [
  { value: '', label: 'All Time' },
  { value: 'today', label: 'Today' },
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' },
  { value: '60d', label: 'Last 60 Days' },
  { value: '90d', label: 'Last 90 Days' },
]

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

  const providerOptions = useMemo(
    () =>
      filters.provider?.options.length
        ? [
            { value: '', label: 'All Providers' },
            ...filters.provider.options.map((p) => ({ value: p, label: p })),
          ]
        : [],
    [filters.provider?.options]
  )

  const organizationOptions = useMemo(
    () =>
      filters.organization?.options.length
        ? [
            { value: '', label: 'All Organizations' },
            ...filters.organization.options.map((o) => ({ value: o, label: o })),
          ]
        : [],
    [filters.organization?.options]
  )

  const repositoryOptions = useMemo(
    () =>
      filters.repository?.options.length
        ? [
            { value: '', label: 'All Repositories' },
            ...filters.repository.options.map((r) => ({ value: r, label: r })),
          ]
        : [],
    [filters.repository?.options]
  )

  const workflowOptions = useMemo(
    () =>
      filters.workflow?.options.length
        ? [
            { value: '', label: 'All Workflows' },
            ...filters.workflow.options.map((w) => ({ value: w, label: w })),
          ]
        : [],
    [filters.workflow?.options]
  )

  const branchOptions = useMemo(
    () =>
      filters.branch?.options.length
        ? [
            { value: '', label: 'All Branches' },
            ...filters.branch.options.map((b) => ({ value: b, label: b })),
          ]
        : [],
    [filters.branch?.options]
  )

  const actorOptions = useMemo(
    () =>
      filters.actor?.options.length
        ? [
            { value: '', label: 'All Actors' },
            ...filters.actor.options.map((a) => ({ value: a, label: a })),
          ]
        : [],
    [filters.actor?.options]
  )

  const hasActiveFilters = useMemo(
    () =>
      !!(
        (filters.search && filters.search.value) ||
        (filters.status && filters.status.value) ||
        (filters.provider && filters.provider.value) ||
        (filters.organization && filters.organization.value) ||
        (filters.repository && filters.repository.value) ||
        (filters.workflow && filters.workflow.value) ||
        (filters.branch && filters.branch.value) ||
        (filters.actor && filters.actor.value) ||
        (filters.dateRange && filters.dateRange.value)
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
            placeholder={filters.search.placeholder || 'Search...'}
            leftSection={<IconSearch size={14} />}
            rightSection={
              localSearchValue && (
                <ActionIcon
                  size="xs"
                  variant="transparent"
                  onClick={() => setLocalSearchValue('')}
                >
                  <IconX size={12} />
                </ActionIcon>
              )
            }
            value={localSearchValue}
            onChange={(e) => setLocalSearchValue(e.currentTarget.value)}
            style={{ minWidth: 200, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.status && (
          <Select
            placeholder="Status"
            data={statusOptions}
            value={filters.status.value}
            onChange={filters.status.onChange}
            clearable
            leftSection={<IconFilter size={14} />}
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.provider && providerOptions.length > 0 && (
          <Select
            placeholder="Provider"
            data={providerOptions}
            value={filters.provider.value}
            onChange={filters.provider.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.organization && organizationOptions.length > 0 && (
          <Select
            placeholder="Organization"
            data={organizationOptions}
            value={filters.organization.value}
            onChange={filters.organization.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.repository && repositoryOptions.length > 0 && (
          <Select
            placeholder="Repository"
            data={repositoryOptions}
            value={filters.repository.value}
            onChange={filters.repository.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.workflow && workflowOptions.length > 0 && (
          <Select
            placeholder="Workflow"
            data={workflowOptions}
            value={filters.workflow.value}
            onChange={filters.workflow.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.branch && branchOptions.length > 0 && (
          <Select
            placeholder="Branch"
            data={branchOptions}
            value={filters.branch.value}
            onChange={filters.branch.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.actor && actorOptions.length > 0 && (
          <Select
            placeholder="Actor"
            data={actorOptions}
            value={filters.actor.value}
            onChange={filters.actor.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.dateRange && (
          <Select
            placeholder="Time Range"
            data={dateRangeOptions}
            value={filters.dateRange.value}
            onChange={filters.dateRange.onChange}
            clearable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {hasActiveFilters && (
          <Button
            variant="subtle"
            color="gray"
            onClick={clearAllFilters}
            title="Clear all filters"
            size="xs"
            leftSection={<IconX size={14} />}
          >
            Clear Filters
          </Button>
        )}
      </Group>
    </Box>
  )
}
