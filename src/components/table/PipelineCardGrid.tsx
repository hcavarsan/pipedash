import { Card, Checkbox, Group, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import type { ProviderSummary, ViewPipeline } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { useContextMenu } from '../common/ContextMenu'
import { IosGroup } from '../ios/IosList'

import { IosPipelineRow, relativeOrNever } from './IosPipelineList'
import {
  FetchInfo,
  type PipelineActions,
  type PipelineSelection,
  WorkflowActionMenu,
  WorkflowActions,
  workflowMenuItems,
  workflowSwipeActions,
} from './PipelineRowParts'

interface PipelineCardGridProps {
  header?: string
  headerAction?: ReactNode
  pipelines: ViewPipeline[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection
}

export function PipelineCardGrid({
  header,
  headerAction,
  pipelines,
  getProvider,
  actions,
  selection,
}: PipelineCardGridProps) {
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const contextMenu = useContextMenu()

  if (mobileShell) {
    return (
      <IosGroup header={header} headerAction={headerAction}>
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
              selected={selection ? selection.selected.has(pipeline.id) : undefined}
              onClick={() => (selection ? selection.onToggle(pipeline) : actions.onViewHistory(pipeline))}
              swipeActions={workflowSwipeActions(pipeline, actions)}
              trailing={<WorkflowActionMenu pipeline={pipeline} provider={provider} actions={actions} />}
            />
          )
        })}
      </IosGroup>
    )
  }

  return (
    <SimpleGrid cols={{ base: 1, sm: 2, lg: 3, xl: 4 }} spacing="sm">
      {pipelines.map((pipeline) => (
        <Card
          key={pipeline.id}
          padding="sm"
          className="interactive-surface stretched"
          {...contextMenu(() => workflowMenuItems(pipeline, actions))}
        >
          <Stack gap="xs" h="100%" justify="space-between">
            <Group justify="space-between" align="flex-start" wrap="nowrap" gap="sm">
              {selection && (
                <Checkbox
                  mt={2}
                  aria-label={`Select ${pipeline.name}`}
                  checked={selection.selected.has(pipeline.id)}
                  onChange={() => selection.onToggle(pipeline)}
                />
              )}
              <UnstyledButton
                className="item-primary"
                onClick={() => actions.onViewHistory(pipeline)}
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
              <FetchInfo
                lead={getStatusLabel(pipeline)}
                providers={[getProvider(pipeline.provider_id)].flatMap((provider) => provider ?? [])}
              />
            </Group>

            <Group justify="space-between" wrap="nowrap" gap="sm">
              <Text size="xs" c="dimmed" truncate>
                {getProvider(pipeline.provider_id)?.name ?? 'Unknown'} ·{' '}
                {pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never run'}
              </Text>
              <WorkflowActions pipeline={pipeline} actions={actions} />
            </Group>
          </Stack>
        </Card>
      ))}
    </SimpleGrid>
  )
}
