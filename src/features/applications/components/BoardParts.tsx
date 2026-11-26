import { useDndContext, useDraggable, useDroppable } from '@dnd-kit/core'
import { Box, Group, Paper, Stack, Text } from '@mantine/core'
import { IconGripVertical } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { StatusGlyph } from '../../../components/atoms/StatusBadge'
import type { BoardCard, BoardLane, BoardPipeline, BoardTarget, BoardZone } from '../../../types/applicationBoard'

export interface DraggedCard {
  card: BoardCard
  label: string
}

const hasField = (data: unknown, field: string): boolean => typeof data === 'object' && data !== null && field in data

const isDraggedCard = (data: unknown): data is DraggedCard => hasField(data, 'card')
const isBoardZone = (data: unknown): data is BoardZone => hasField(data, 'target')

export function draggedCardOf(data: unknown): DraggedCard | null {
  return isDraggedCard(data) ? data : null
}

export function boardZoneOf(data: unknown): BoardZone | null {
  return isBoardZone(data) ? data : null
}

interface DraggableCardProps {
  card: BoardCard
  label: string
  busy: boolean
  children: ReactNode
}

function DraggableCard({ card, label, busy, children }: DraggableCardProps): ReactNode {
  const disabled = busy || !card.draggable
  const dragged: DraggedCard = { card, label }
  const { setNodeRef, attributes, listeners, isDragging } = useDraggable({
    id: JSON.stringify(card.item),
    data: dragged,
    disabled,
  })

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
        {card.draggable && <IconGripVertical size={14} aria-hidden color="var(--mantine-color-dimmed)" />}
        {children}
      </Group>
    </Paper>
  )
}

export function PipelineCard({ pipeline, busy }: { pipeline: BoardPipeline; busy: boolean }): ReactNode {
  return (
    <DraggableCard card={pipeline.card} label={pipeline.name} busy={busy}>
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

export function TargetCard({ target, busy }: { target: BoardTarget; busy: boolean }): ReactNode {
  return (
    <DraggableCard card={target.card} label={target.name} busy={busy}>
      <Text size="sm" truncate>
        {target.name}
      </Text>
    </DraggableCard>
  )
}

interface DropAreaProps {
  zone: BoardZone
  busy: boolean
  children?: ReactNode
}

export function DropArea({ zone, busy, children }: DropAreaProps): ReactNode {
  const { active } = useDndContext()
  const dragged = draggedCardOf(active?.data.current)?.card
  const accepts =
    !busy &&
    zone.enabled &&
    dragged !== undefined &&
    dragged.drop_class === zone.drop_class &&
    dragged.zone_id !== zone.id
  const { setNodeRef, isOver } = useDroppable({ id: zone.id, data: zone, disabled: !accepts })
  let background = 'transparent'

  if (accepts) {
    background = isOver ? 'var(--mantine-color-blue-light)' : 'var(--mantine-color-default-hover)'
  }

  return (
    <Box
      ref={setNodeRef}
      role="group"
      aria-label={zone.label}
      data-drop-zone={zone.id}
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
        {zone.hint && (
          <Text size="xs" c="dimmed">
            {zone.hint}
          </Text>
        )}
      </Stack>
    </Box>
  )
}

export function LaneArea({ lane, busy }: { lane: BoardLane; busy: boolean }): ReactNode {
  return (
    <DropArea zone={lane.zone} busy={busy}>
      {lane.pipelines.map((pipeline) => (
        <PipelineCard key={pipeline.card.item.key} pipeline={pipeline} busy={busy} />
      ))}
    </DropArea>
  )
}
