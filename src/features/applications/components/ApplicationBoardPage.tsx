import {
  type CollisionDetection,
  DndContext,
  type DragEndEvent,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  rectIntersection,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import { Group, Loader, Paper, Stack, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { PageHeader } from '../../../components/common/PageHeader'
import { SearchInput } from '../../../components/common/SearchInput'
import type { ApplicationBoard, BoardDropOutcome } from '../../../types/applicationBoard'
import type { ApplicationCopy } from '../../../types/applications'
import { useBoardDrop } from '../queries/useApplicationQueries'
import { ApplicationsColumn, UnassignedColumn } from './BoardColumns'
import { boardZoneOf, draggedCardOf } from './BoardParts'
import { CreateApplicationModal } from './CreateApplicationModal'
import { NewEnvironmentModal } from './NewEnvironmentModal'

interface ApplicationBoardPageProps {
  board: ApplicationBoard
  applicationCopy: ApplicationCopy
  search: string
  onSearchChange: (search: string) => void
  onBack: () => void
}

type Prompt = Exclude<BoardDropOutcome, { type: 'applied' }>

const UNASSIGNED_WIDTH = 300

const collisions: CollisionDetection = (args) => {
  const underPointer = pointerWithin(args)

  return underPointer.length > 0 ? underPointer : rectIntersection(args)
}

export function ApplicationBoardPage({
  board,
  applicationCopy,
  search,
  onSearchChange,
  onBack,
}: ApplicationBoardPageProps): ReactNode {
  const { copy } = board
  const drop = useBoardDrop()
  const [dragged, setDragged] = useState<string | null>(null)
  const [prompt, setPrompt] = useState<Prompt | null>(null)
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor)
  )
  const busy = drop.isPending

  const onDragEnd = ({ active, over }: DragEndEvent): void => {
    setDragged(null)
    const card = draggedCardOf(active.data.current)?.card
    const zone = boardZoneOf(over?.data.current)

    if (busy || card === undefined || zone === null) {
      return
    }

    drop.mutate(
      { card: card.item, zone: zone.target },
      { onSuccess: (outcome) => setPrompt(outcome.type === 'applied' ? null : outcome) }
    )
  }

  return (
    <Stack gap="md">
      <PageHeader
        title={copy.title}
        onBack={onBack}
        backLabel={copy.back}
        actions={busy ? <Loader size="xs" aria-label={copy.title} /> : undefined}
      />
      <Text size="sm" c="dimmed">
        {copy.help}
      </Text>
      <SearchInput placeholder={copy.search_placeholder} value={search} onChange={onSearchChange} />
      <DndContext
        sensors={sensors}
        collisionDetection={collisions}
        onDragStart={({ active }) => setDragged(draggedCardOf(active.data.current)?.label ?? null)}
        onDragCancel={() => setDragged(null)}
        onDragEnd={onDragEnd}
      >
        <Group gap="md" align="flex-start" wrap="nowrap">
          <UnassignedColumn lane={board.unassigned} busy={busy} width={UNASSIGNED_WIDTH} />
          <ApplicationsColumn board={board} busy={busy} />
        </Group>
        <DragOverlay dropAnimation={null}>
          {dragged !== null && (
            <Paper withBorder radius="sm" px="xs" py="2xs" shadow="md">
              <Text size="sm">{dragged}</Text>
            </Paper>
          )}
        </DragOverlay>
      </DndContext>
      {prompt?.type === 'name_application' && (
        <CreateApplicationModal
          opened
          copy={applicationCopy}
          initialMembers={[prompt.pipeline_id]}
          onCreated={() => undefined}
          onClose={() => setPrompt(null)}
        />
      )}
      {prompt?.type === 'name_environment' && (
        <NewEnvironmentModal
          applicationId={prompt.application_id}
          targetKey={prompt.target_key}
          copy={copy}
          onClose={() => setPrompt(null)}
        />
      )}
    </Stack>
  )
}
