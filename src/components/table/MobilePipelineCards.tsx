import { Box, Card, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronRight, IconFolder, IconGitBranch } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { TYPOGRAPHY } from '../../theme/typography'
import type { Pipeline, ProviderSummary } from '../../types'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusBadge } from '../atoms/StatusBadge'
import { ProviderStatusButton } from '../provider/ProviderStatusButton'

import { RepositoryFetchIndicator, type TableRow, WorkflowActions } from './PipelineRowParts'

interface MobilePipelineCardsProps {
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  pinnedKeys: string[]
  onTogglePin: (pinKey: string) => void
  hiddenKeys: string[]
  onToggleHidden: (pinKey: string) => void
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
  onViewHistory,
  onTrigger,
  pinnedKeys,
  onTogglePin,
  hiddenKeys,
  onToggleHidden,
  onViewMetrics,
}: MobilePipelineCardsProps) {
  const renderProviders = (row: TableRow) => (
    <Group gap="xs" px="xs" wrap="wrap">
      {Array.from(row.providerIds ?? []).map((providerId) => (
        <ProviderStatusButton key={providerId} provider={getProvider(providerId)} />
      ))}
    </Group>
  )

  return (
    <Stack gap="sm">
      {rows.map((row) => {
        if (row.type === 'repository') {
          const isFluxGroup =
            !!row.providerIds?.size &&
            Array.from(row.providerIds).every((id) => getProvider(id)?.provider_type === 'fluxcd')
          return (
            <Card key={row.id} padding="xs">
              <Stack gap={4}>
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
                        {row.status && <StatusBadge status={row.status} label={row.statusLabel} />}
                      </Group>

                      <Group gap="md" wrap="wrap">
                        <Field label={isFluxGroup ? 'Namespace' : 'Organization'}>{row.organization}</Field>
                        <Field label={isFluxGroup ? 'Resources' : 'Workflows'}>{row.pipelineCount}</Field>
                      </Group>
                    </Stack>
                  </UnstyledButton>
                  <RepositoryFetchIndicator row={row} />
                </Group>
                {renderProviders(row)}
              </Stack>
            </Card>
          )
        }

        const { pipeline, pinKey } = row

        if (!pipeline || !pinKey) {
          return null
        }

        return (
          <Card key={row.id} padding="xs" ml="xl">
            <Stack gap="xs">
              <UnstyledButton
                className="interactive-surface"
                onClick={() => onViewHistory(pipeline)}
                aria-label={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()} for ${row.name}`}
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
                    {row.status && <StatusBadge status={row.status} label={getStatusLabel(pipeline)} />}
                  </Group>

                  <Group gap="md" wrap="wrap">
                    <Field label={getPipelineLabels(pipeline.provider_type).groupTitle}>{row.organization}</Field>
                    {pipeline.branch && <Field label="Branch">{pipeline.branch}</Field>}
                  </Group>
                </Stack>
              </UnstyledButton>
              <Group justify="space-between" wrap="nowrap">
                {renderProviders(row)}
                <WorkflowActions
                  pipeline={pipeline}
                  isPinned={pinnedKeys.includes(pinKey)}
                  onTogglePin={() => onTogglePin(pinKey)}
                  isHidden={hiddenKeys.includes(pinKey)}
                  onToggleHidden={() => onToggleHidden(pinKey)}
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
