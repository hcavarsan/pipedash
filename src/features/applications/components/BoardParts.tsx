import { useDndContext, useDraggable, useDroppable } from '@dnd-kit/core'
import { Box, Group, Paper, Stack, Text } from '@mantine/core'
import { IconGripVertical } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { StatusGlyph } from '../../../components/atoms/StatusBadge'
import type { BoardPipeline, BoardTarget } from '../../../types/applicationBoard'
import type { Stage } from '../../../types/applications'
import { type DragItem, type DropZone, dropAction, zoneId } from '../lib/boardDrop'

const DRAG_KINDS = ['pipeline', 'target']
const ZONE_KINDS = ['unassigned', 'new_application', 'lane', 'environment', 'new_environment']

const hasKind = (data: unknown, kinds: string[]): boolean =>
  typeof data === 'object' &&
  data !== null &&
  'kind' in data &&
  typeof data.kind === 'string' &&
  kinds.includes(data.kind)

const isDragItem = (data: unknown): data is DragItem => hasKind(data, DRAG_KINDS)
const isDropZone = (data: unknown): data is DropZone => hasKind(data, ZONE_KINDS)

export function dragItemOf(data: unknown): DragItem | null {
  return isDragItem(data) ? data : null
}

export function dropZoneOf(data: unknown): DropZone | null {
  return isDropZone(data) ? data : null
}

interface DraggableCardProps {
  id: string
  item: DragItem
  disabled: boolean
  children: ReactNode
}

function DraggableCard({ id, item, disabled, children }: DraggableCardProps): ReactNode {
  const { setNodeRef, attributes, listeners, isDragging } = useDraggable({ id, data: item, disabled })

  return (
    <Paper
      ref={setNodeRef}
      withBorder
      radius="sm"
      px="xs"
      py="2xs"
      className="interactive-surface"
      opacity={isDragging ? 0.4 : 1}
      style={{ cursor: disabled ? 'default' : 'grab', touchAction: 'none' }}
      {...attributes}
      {...listeners}
    >
      <Group gap="xs" wrap="nowrap">
        {!disabled && <IconGripVertical size={14} aria-hidden color="var(--mantine-color-dimmed)" />}
        {children}
      </Group>
    </Paper>
  )
}

interface PipelineCardProps {
  pipeline: BoardPipeline
  applicationId: number | null
  stage: Stage | null
  disabled: boolean
}

export function PipelineCard({ pipeline, applicationId, stage, disabled }: PipelineCardProps): ReactNode {
  const item: DragItem = {
    kind: 'pipeline',
    label: pipeline.name,
    pipelineId: pipeline.pipeline_id,
    applicationId,
    stage,
  }

  return (
    <DraggableCard id={`pipeline:${applicationId ?? 'none'}:${pipeline.pipeline_id}`} item={item} disabled={disabled}>
      <StatusGlyph status={pipeline.status} />
      <Stack gap={0} miw={0} flex={1}>
        <Text size="sm" truncate>
          {pipeline.name}
        </Text>
        <Text size="xs" c="dimmed" truncate>
          {pipeline.repository}
        </Text>
      </Stack>
    </DraggableCard>
  )
}

interface TargetCardProps {
  target: BoardTarget
  applicationId: number
  groupKey: string
  disabled: boolean
}

export function TargetCard({ target, applicationId, groupKey, disabled }: TargetCardProps): ReactNode {
  const item: DragItem = { kind: 'target', label: target.name, targetKey: target.target_key, applicationId, groupKey }

  return (
    <DraggableCard id={`target:${applicationId}:${target.target_key}`} item={item} disabled={disabled}>
      <Text size="sm" truncate>
        {target.name}
      </Text>
    </DraggableCard>
  )
}

interface DropAreaProps {
  zone: DropZone
  label: string
  disabled?: boolean
  hint?: string | undefined
  children?: ReactNode
}

export function DropArea({ zone, label, disabled = false, hint, children }: DropAreaProps): ReactNode {
  const id = zoneId(zone)
  const { active } = useDndContext()
  const dragged = dragItemOf(active?.data.current)
  const accepts = !disabled && dragged !== null && dropAction(dragged, zone) !== null
  const { setNodeRef, isOver } = useDroppable({ id, data: zone, disabled: !accepts })
  let background = 'transparent'

  if (accepts) {
    background = isOver ? 'var(--mantine-color-blue-light)' : 'var(--mantine-color-default-hover)'
  }

  return (
    <Box
      ref={setNodeRef}
      role="group"
      aria-label={label}
      data-drop-zone={id}
      data-accepts={accepts || undefined}
      p="xs"
      mih={56}
      bg={background}
      style={{
        border: `1px ${accepts ? 'dashed' : 'solid'} var(--mantine-color-default-border)`,
        borderRadius: 'var(--mantine-radius-sm)',
      }}
    >
      <Stack gap="2xs">
        {children}
        {hint && (
          <Text size="xs" c="dimmed">
            {hint}
          </Text>
        )}
      </Stack>
    </Box>
  )
}
