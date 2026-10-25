import { Card, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import type { Pipeline, ProviderSummary } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusIcon } from '../atoms/StatusBadge'
import { useContextMenu } from '../common/ContextMenu'
import { IosGroup } from '../ios/IosList'

import { IosPipelineRow, relativeOrNever } from './IosPipelineList'
import { WorkflowActionMenu, WorkflowActions, workflowMenuItems, workflowSwipeActions } from './PipelineRowParts'

interface PinnedWorkflowsProps {
  header?: string
  pipelines: Pipeline[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  onUnpin: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function PinnedWorkflows({
  header,
  pipelines,
  getProvider,
  onViewHistory,
  onTrigger,
  onUnpin,
  onViewMetrics,
}: PinnedWorkflowsProps) {
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const contextMenu = useContextMenu()
  const pinnedMenu = (pipeline: Pipeline) =>
    contextMenu(() =>
      workflowMenuItems({
        pipeline,
        isPinned: true,
        onTogglePin: () => onUnpin(pipeline),
        onTrigger,
        onViewMetrics,
        onViewHistory,
      })
    )

  if (mobileShell) {
    return (
      <IosGroup header={header}>
        {pipelines.map((pipeline) => {
          const provider = getProvider(pipeline.provider_id)

          return (
            <IosPipelineRow
              key={pipeline.id}
              status={pipeline.status}
              statusText={getStatusLabel(pipeline)}
              title={pipeline.name}
              subtitle={[pipeline.repository, provider?.name ?? 'Unknown'].join(' · ')}
              value={relativeOrNever(pipeline.last_run, now)}
              hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
              longPressMenu
              onClick={() => onViewHistory(pipeline)}
              swipeActions={workflowSwipeActions(pipeline, true, () => onUnpin(pipeline), onTrigger)}
              trailing={
                <WorkflowActionMenu
                  pipeline={pipeline}
                  provider={provider}
                  isPinned
                  onTogglePin={() => onUnpin(pipeline)}
                  onTrigger={onTrigger}
                  onViewMetrics={onViewMetrics}
                  onViewHistory={onViewHistory}
                />
              }
            />
          )
        })}
      </IosGroup>
    )
  }

  return (
    <SimpleGrid cols={{ base: 1, sm: 2, lg: 3, xl: 4 }} spacing="sm">
      {pipelines.map((pipeline) => (
        <Card key={pipeline.id} padding="sm" {...pinnedMenu(pipeline)}>
          <Stack gap="xs" h="100%" justify="space-between">
            <Group justify="space-between" align="flex-start" wrap="nowrap" gap="sm">
              <UnstyledButton
                className="interactive-surface"
                onClick={() => onViewHistory(pipeline)}
                aria-label={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()} for ${pipeline.name}`}
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
              <StatusIcon status={pipeline.status} label={getStatusLabel(pipeline)} />
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
