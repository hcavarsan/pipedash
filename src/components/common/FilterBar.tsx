import { ActionIcon, Box, Group, Select, TextInput } from '@mantine/core'
import { IconFilter, IconSearch, IconX } from '@tabler/icons-react'

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

export const FilterBar = ({ filters }: FilterBarProps) => {
  const hasActiveFilters =
    (filters.search && filters.search.value) ||
    (filters.status && filters.status.value) ||
    (filters.provider && filters.provider.value) ||
    (filters.organization && filters.organization.value) ||
    (filters.repository && filters.repository.value) ||
    (filters.workflow && filters.workflow.value) ||
    (filters.branch && filters.branch.value) ||
    (filters.actor && filters.actor.value) ||
    (filters.dateRange && filters.dateRange.value)

  const clearAllFilters = () => {
    filters.search?.onChange('')
    filters.status?.onChange(null)
    filters.provider?.onChange(null)
    filters.organization?.onChange(null)
    filters.repository?.onChange(null)
    filters.workflow?.onChange(null)
    filters.branch?.onChange(null)
    filters.actor?.onChange(null)
    filters.dateRange?.onChange(null)
  }

  return (
    <Box mb={4}>
      <Group gap="xs" wrap="wrap" align="flex-start">
        {filters.search && (
          <TextInput
            placeholder={filters.search.placeholder || 'Search...'}
            leftSection={<IconSearch size={14} />}
            rightSection={
              filters.search.value && (
                <ActionIcon
                  size="xs"
                  variant="transparent"
                  onClick={() => filters.search!.onChange('')}
                >
                  <IconX size={12} />
                </ActionIcon>
              )
            }
            value={filters.search.value}
            onChange={(e) => filters.search!.onChange(e.currentTarget.value)}
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

        {filters.provider && filters.provider.options.length > 0 && (
          <Select
            placeholder="Provider"
            data={[
              { value: '', label: 'All Providers' },
              ...filters.provider.options.map((p) => ({ value: p, label: p })),
            ]}
            value={filters.provider.value}
            onChange={filters.provider.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.organization && filters.organization.options.length > 0 && (
          <Select
            placeholder="Organization"
            data={[
              { value: '', label: 'All Organizations' },
              ...filters.organization.options.map((o) => ({ value: o, label: o })),
            ]}
            value={filters.organization.value}
            onChange={filters.organization.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.repository && filters.repository.options.length > 0 && (
          <Select
            placeholder="Repository"
            data={[
              { value: '', label: 'All Repositories' },
              ...filters.repository.options.map((r) => ({ value: r, label: r })),
            ]}
            value={filters.repository.value}
            onChange={filters.repository.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.workflow && filters.workflow.options.length > 0 && (
          <Select
            placeholder="Workflow"
            data={[
              { value: '', label: 'All Workflows' },
              ...filters.workflow.options.map((w) => ({ value: w, label: w })),
            ]}
            value={filters.workflow.value}
            onChange={filters.workflow.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.branch && filters.branch.options.length > 0 && (
          <Select
            placeholder="Branch"
            data={[
              { value: '', label: 'All Branches' },
              ...filters.branch.options.map((b) => ({ value: b, label: b })),
            ]}
            value={filters.branch.value}
            onChange={filters.branch.onChange}
            clearable
            searchable
            style={{ minWidth: 130, flex: 1, maxWidth: '100%' }}
            size="xs"
          />
        )}

        {filters.actor && filters.actor.options.length > 0 && (
          <Select
            placeholder="Actor"
            data={[
              { value: '', label: 'All Actors' },
              ...filters.actor.options.map((a) => ({ value: a, label: a })),
            ]}
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
          <ActionIcon
            variant="subtle"
            color="gray"
            onClick={clearAllFilters}
            title="Clear all filters"
            size="md"
          >
            <IconX size={16} />
          </ActionIcon>
        )}
      </Group>
    </Box>
  )
}
