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
import type { ApplicationBoard } from '../../../types/applicationBoard'
import type { ApplicationCopy } from '../../../types/applications'
import { type DragItem, type DropAction, dropAction } from '../lib/boardDrop'
import { useEditApplication } from '../queries/useApplicationQueries'
import { ApplicationsColumn, UnassignedColumn } from './BoardColumns'
import { dragItemOf, dropZoneOf } from './BoardParts'
import { CreateApplicationModal } from './CreateApplicationModal'
import { NewEnvironmentModal } from './NewEnvironmentModal'

interface ApplicationBoardPageProps {
  board: ApplicationBoard
  applicationCopy: ApplicationCopy
  search: string
  onSearchChange: (search: string) => void
  onBack: () => void
}

type Prompt = Exclude<DropAction, { kind: 'edit' }>

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
  const { copy, unassigned, applications } = board
  const edit = useEditApplication()
  const [dragged, setDragged] = useState<DragItem | null>(null)
  const [prompt, setPrompt] = useState<Prompt | null>(null)
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor)
  )
  const busy = edit.isPending

  const onDragEnd = ({ active, over }: DragEndEvent): void => {
    setDragged(null)
    const item = dragItemOf(active.data.current)
    const zone = dropZoneOf(over?.data.current)

    if (busy || item === null || zone === null) {
      return
    }

    const action = dropAction(item, zone)

    if (action === null) {
      return
    }
    if (action.kind === 'edit') {
      edit.mutate({ id: action.applicationId, edit: action.edit })

      return
    }

    setPrompt(action)
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
        onDragStart={({ active }) => setDragged(dragItemOf(active.data.current))}
        onDragCancel={() => setDragged(null)}
        onDragEnd={onDragEnd}
      >
        <Group gap="md" align="flex-start" wrap="nowrap">
          <UnassignedColumn pipelines={unassigned} copy={copy} busy={busy} width={UNASSIGNED_WIDTH} />
          <ApplicationsColumn
            applications={applications}
            copy={copy}
            busy={busy}
            newApplicationLabel={applicationCopy.new_application}
          />
        </Group>
        <DragOverlay dropAnimation={null}>
          {dragged && (
            <Paper withBorder radius="sm" px="xs" py="2xs" shadow="md">
              <Text size="sm">{dragged.label}</Text>
            </Paper>
          )}
        </DragOverlay>
      </DndContext>
      {prompt?.kind === 'create_application' && (
        <CreateApplicationModal
          opened
          copy={applicationCopy}
          initialMembers={[prompt.pipelineId]}
          onCreated={() => undefined}
          onClose={() => setPrompt(null)}
        />
      )}
      {prompt?.kind === 'create_environment' && (
        <NewEnvironmentModal
          applicationId={prompt.applicationId}
          targetKey={prompt.targetKey}
          copy={copy}
          onClose={() => setPrompt(null)}
        />
      )}
    </Stack>
  )
}
