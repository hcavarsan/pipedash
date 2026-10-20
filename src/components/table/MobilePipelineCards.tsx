import { Box, Card, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import {
  IconChevronDown,
  IconChevronRight,
  IconFolder,
  IconGitBranch,
  IconPinFilled,
  IconPlugConnected,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { TYPOGRAPHY } from '../../theme/typography'
import type { Pipeline, ProviderSummary } from '../../types'
import { TableCells } from '../../utils/tableCells'

import {
  type ProviderFetchError,
  RepositoryFetchIndicator,
  RowStatus,
  type TableRow,
  WorkflowActions,
} from './PipelineRowParts'

interface MobilePipelineCardsProps {
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  onShowProviderError: (error: ProviderFetchError) => void
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  pinnedKeys: string[]
  onTogglePin: (pinKey: string) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Box style={{ flex: '1 1 auto' }}>
      <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
        {label}
      </Text>
      <Text size={TYPOGRAPHY.FIELD_VALUE.size}>{children}</Text>
    </Box>
  )
}

export function MobilePipelineCards({
  rows,
  getProvider,
  onToggleRepo,
  onShowProviderError,
  onViewHistory,
  onTrigger,
  pinnedKeys,
  onTogglePin,
  onViewMetrics,
}: MobilePipelineCardsProps) {
  const renderProviders = (row: TableRow, label: string) => (
    <Box>
      <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed" mb={4}>
        {label}
      </Text>
      <Stack gap="xs">
        {Array.from(row.providerIds ?? []).map((providerId) => {
          const provider = getProvider(providerId)

          return (
            <div key={providerId}>
              {TableCells.avatarName(
                provider?.icon || null,
                provider?.name || 'Unknown',
                <IconPlugConnected size={14} />
              )}
            </div>
          )
        })}
      </Stack>
    </Box>
  )

  return (
    <Stack gap="sm">
      {rows.map((row) => {
        if (row.type === 'repository') {
          return (
            <Card key={row.id} padding="xs">
              <Group wrap="nowrap" align="flex-start" gap="xs">
                <UnstyledButton
                  className="interactive-surface"
                  onClick={() => onToggleRepo(row.id)}
                  aria-expanded={row.expanded}
                  p="xs"
                  bdrs="sm"
                  style={{ flex: 1, minWidth: 0 }}
                >
                  <Stack gap="xs">
                    <Group justify="space-between" wrap="nowrap">
                      <Group gap="xs" wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
                        {row.expanded ? (
                          <IconChevronDown size={16} style={{ flexShrink: 0 }} />
                        ) : (
                          <IconChevronRight size={16} style={{ flexShrink: 0 }} />
                        )}
                        <IconFolder size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                        <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight} truncate>
                          {row.name}
                        </Text>
                      </Group>
                      <RowStatus row={row} />
                    </Group>

                    <Group gap="md" wrap="wrap">
                      <Field label="Organization">{row.organization}</Field>
                      <Field label="Workflows">{row.pipelineCount}</Field>
                    </Group>

                    {renderProviders(row, 'Providers')}
                  </Stack>
                </UnstyledButton>
                <RepositoryFetchIndicator row={row} onShowError={onShowProviderError} />
              </Group>
            </Card>
          )
        }

        if (row.type === 'pinned-header') {
          return (
            <Group key={row.id} gap="xs" wrap="nowrap">
              <IconPinFilled size={16} color="var(--mantine-color-dimmed)" />
              <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight}>
                Pinned workflows
              </Text>
            </Group>
          )
        }

        const { pipeline, pinKey } = row

        if (!pipeline || !pinKey) {
          return null
        }

        return (
          <Card key={row.id} padding="xs" ml={row.pinned ? undefined : 'xl'}>
            <Stack gap="xs">
              <UnstyledButton
                className="interactive-surface"
                onClick={() => onViewHistory(pipeline)}
                aria-label={`View run history for ${row.name}`}
                p="xs"
                bdrs="sm"
              >
                <Stack gap="xs">
                  <Group justify="space-between" wrap="nowrap">
                    <Group gap="xs" wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
                      <IconGitBranch size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight} truncate>
                        {row.name}
                      </Text>
                    </Group>
                    <RowStatus row={row} />
                  </Group>

                  <Group gap="md" wrap="wrap">
                    <Field label="Organization">{row.organization}</Field>
                    {pipeline.branch && <Field label="Branch">{pipeline.branch}</Field>}
                  </Group>

                  {renderProviders(row, 'Provider')}
                </Stack>
              </UnstyledButton>
              <Group justify="flex-end">
                <WorkflowActions
                  pipeline={pipeline}
                  isPinned={pinnedKeys.includes(pinKey)}
                  onTogglePin={() => onTogglePin(pinKey)}
                  onTrigger={onTrigger}
                  onViewMetrics={onViewMetrics}
                />
              </Group>
            </Stack>
          </Card>
        )
      })}
    </Stack>
  )
}
