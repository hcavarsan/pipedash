import { MultiSelect, Stack, TagsInput, Text, TextInput, Title } from '@mantine/core'
import type { ReactNode } from 'react'

import { STATUS_OPTIONS } from '../../../theme/status'
import type { PipelineStatus, ProviderSummary, ViewRules } from '../../../types/domain'

interface ViewRulesFieldsProps {
  rules: ViewRules
  providers: ProviderSummary[]
  onChange: (patch: Partial<ViewRules>) => void
}

export function ViewRulesFields({ rules, providers, onChange }: ViewRulesFieldsProps): ReactNode {
  return (
    <Stack gap="sm">
      <div>
        <Title order={3} size="sm">
          Auto-include
        </Title>
        <Text size="xs" c="dimmed">
          Pipelines that match every filled rule join this view automatically.
        </Text>
      </div>
      <MultiSelect
        label="Providers"
        data={providers.map((p) => ({ value: String(p.id), label: p.name }))}
        value={rules.provider_ids.map(String)}
        onChange={(values) => onChange({ provider_ids: values.map(Number) })}
        clearable
        searchable
      />
      <TagsInput
        label="Repositories"
        placeholder="acme/* or acme/app"
        value={rules.repositories}
        onChange={(repositories) => onChange({ repositories })}
        clearable
      />
      <TextInput
        label="Name contains"
        value={rules.name_contains ?? ''}
        onChange={(event) => onChange({ name_contains: event.currentTarget.value || null })}
      />
      <MultiSelect<PipelineStatus>
        label="Statuses"
        data={STATUS_OPTIONS}
        value={rules.statuses}
        onChange={(values) => onChange({ statuses: values })}
        clearable
      />
    </Stack>
  )
}
