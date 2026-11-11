import { ActionIcon, Group, MultiSelect, Select, Switch } from '@mantine/core'
import { IconSortAscending, IconSortDescending } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { SearchInput } from '../../../components/common/SearchInput'
import { STATUS_META } from '../../../theme/status'
import type { ApplicationCopy, ApplicationListQuery, ApplicationSort } from '../../../types/applications'
import type { ProviderSummary } from '../../../types/providers'
import { typedKeys } from '../../../utils/typedKeys'

interface ApplicationsFiltersProps {
  query: ApplicationListQuery
  copy: ApplicationCopy
  providers: ProviderSummary[]
  onChange: (query: ApplicationListQuery) => void
}

const STATUSES = typedKeys(STATUS_META)
const SORTS: ApplicationSort[] = ['name', 'health', 'behind', 'last_deploy']

export function ApplicationsFilters({ query, copy, providers, onChange }: ApplicationsFiltersProps): ReactNode {
  const DirectionIcon = query.direction === 'asc' ? IconSortAscending : IconSortDescending

  return (
    <Group gap="sm" wrap="wrap" align="center">
      <SearchInput
        placeholder={copy.search_placeholder}
        value={query.search ?? ''}
        onChange={(value) => onChange({ ...query, search: value.trim() === '' ? null : value })}
        style={{ flex: '1 1 200px' }}
      />
      <MultiSelect
        aria-label={copy.health_filter}
        placeholder={query.health.length > 0 ? undefined : copy.health_filter}
        data={copy.health_options}
        value={query.health}
        clearable
        flex="1 1 140px"
        onChange={(values) => onChange({ ...query, health: STATUSES.filter((status) => values.includes(status)) })}
      />
      <MultiSelect
        aria-label={copy.provider_filter}
        placeholder={query.provider_ids.length > 0 ? undefined : copy.provider_filter}
        data={providers.map((provider) => ({ value: String(provider.id), label: provider.name }))}
        value={query.provider_ids.map(String)}
        clearable
        flex="1 1 140px"
        onChange={(values) => onChange({ ...query, provider_ids: values.map(Number) })}
      />
      <Select
        aria-label={copy.sort}
        data={copy.sort_options}
        value={query.sort}
        allowDeselect={false}
        w={150}
        onChange={(value) => onChange({ ...query, sort: SORTS.find((sort) => sort === value) ?? query.sort })}
      />
      <ActionIcon
        variant="default"
        size="lg"
        aria-label={query.direction === 'asc' ? copy.sort_ascending : copy.sort_descending}
        onClick={() => onChange({ ...query, direction: query.direction === 'asc' ? 'desc' : 'asc' })}
      >
        <DirectionIcon size={16} aria-hidden />
      </ActionIcon>
      <Switch
        size="sm"
        label={copy.behind_only}
        checked={query.behind_only}
        onChange={(event) => onChange({ ...query, behind_only: event.currentTarget.checked })}
      />
      <Switch
        size="sm"
        label={copy.with_suggestions}
        checked={query.with_suggestions}
        onChange={(event) => onChange({ ...query, with_suggestions: event.currentTarget.checked })}
      />
      <Switch
        size="sm"
        label={copy.include_hidden}
        checked={query.include_hidden}
        onChange={(event) => onChange({ ...query, include_hidden: event.currentTarget.checked })}
      />
    </Group>
  )
}
