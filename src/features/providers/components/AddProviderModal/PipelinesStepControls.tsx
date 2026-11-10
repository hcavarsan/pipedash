import { Group, NativeSelect, Paper, Select } from '@mantine/core'
import type { ReactNode } from 'react'
import { PhoneSearchField } from '../../../../components/common/PhoneSearchField'
import { SearchInput } from '../../../../components/common/SearchInput'
import shell from '../../../../components/forms/ShellForm.module.css'
import type { Organization } from '../../../../types/pipelines'
import type { PipelineTerminology } from '../../../../types/presentation'
import { ShellField } from './ShellForm'

interface PipelinesControlsProps {
  terms: PipelineTerminology
  organizations: Organization[]
  effectiveOrganization: string
  search: string
  onOrganizationSelect: (org: string | null) => void
  onSearchChange: (value: string) => void
}

export function ShellPipelineControls({
  terms,
  organizations,
  effectiveOrganization,
  search,
  onOrganizationSelect,
  onSearchChange,
}: PipelinesControlsProps): ReactNode {
  return (
    <>
      {organizations.length > 1 && (
        <ShellField label={terms.group_title}>
          <NativeSelect
            variant="unstyled"
            aria-label={terms.group_title}
            value={effectiveOrganization}
            onChange={(event) => onOrganizationSelect(event.currentTarget.value)}
            data={[
              { value: '', label: `Select ${terms.group}` },
              ...organizations.map((org) => ({ value: org.id, label: org.name })),
            ]}
          />
        </ShellField>
      )}
      {effectiveOrganization && (
        <div className={shell.search}>
          <PhoneSearchField
            value={search}
            placeholder={`Search ${terms.items}`}
            onChange={onSearchChange}
            onClear={() => onSearchChange('')}
            cancelable={false}
          />
        </div>
      )}
    </>
  )
}

export function DesktopPipelineControls({
  terms,
  organizations,
  effectiveOrganization,
  search,
  searching,
  onOrganizationSelect,
  onSearchChange,
}: PipelinesControlsProps & { searching: boolean }): ReactNode {
  return (
    <Paper p="sm" style={{ flexShrink: 0, marginTop: 'var(--mantine-spacing-md)' }}>
      <Group gap="sm" wrap="nowrap" align="flex-start">
        {organizations.length > 1 && (
          <Select
            aria-label={terms.group_title}
            placeholder={`Select ${terms.group}`}
            value={effectiveOrganization || null}
            onChange={onOrganizationSelect}
            data={organizations.map((org) => ({ value: org.id, label: org.name }))}
            searchable
            style={{ flex: 1 }}
          />
        )}
        <SearchInput
          aria-label={`Search ${terms.items}`}
          placeholder={`Search ${terms.items}…`}
          value={search}
          onChange={onSearchChange}
          loading={searching}
          loadingLabel={`Searching ${terms.items}`}
          disabled={!effectiveOrganization}
          style={{ flex: 1 }}
        />
      </Group>
    </Paper>
  )
}
