import { Card, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'

import type { Pipeline, ProviderSummary } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { StatusIcon } from '../atoms/StatusBadge'

import { WorkflowActions } from './PipelineRowParts'

interface PinnedWorkflowsProps {
  pipelines: Pipeline[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  onUnpin: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function PinnedWorkflows({
  pipelines,
  getProvider,
  onViewHistory,
  onTrigger,
  onUnpin,
  onViewMetrics,
}: PinnedWorkflowsProps) {
  return (
    <SimpleGrid cols={{ base: 1, sm: 2, lg: 3, xl: 4 }} spacing="sm">
      {pipelines.map((pipeline) => (
        <Card key={pipeline.id} padding="sm">
          <Stack gap="xs" h="100%" justify="space-between">
            <Group justify="space-between" align="flex-start" wrap="nowrap" gap="sm">
              <UnstyledButton
                onClick={() => onViewHistory(pipeline)}
                aria-label={`View run history for ${pipeline.name}`}
                miw={0}
                flex={1}
              >
                <Text size="sm" fw={600} truncate>
                  {pipeline.name}
                </Text>
                <Text size="xs" c="dimmed" truncate>
                  {pipeline.repository}
                </Text>
              </UnstyledButton>
              <StatusIcon status={pipeline.status} />
            </Group>

            <Group justify="space-between" wrap="nowrap" gap="sm">
              <Text size="xs" c="dimmed" truncate>
                {getProvider(pipeline.provider_id)?.name ?? 'Unknown'} ·{' '}
                {pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never run'}
              </Text>
              <WorkflowActions
                pipeline={pipeline}
                isPinned
                onTogglePin={() => onUnpin(pipeline)}
                onTrigger={onTrigger}
                onViewMetrics={onViewMetrics}
              />
            </Group>
          </Stack>
        </Card>
      ))}
    </SimpleGrid>
  )
}
