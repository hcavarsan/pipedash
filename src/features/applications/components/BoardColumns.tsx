import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { ApplicationBoard, BoardLane } from '../../../types/applicationBoard'
import { BoardApplicationCard } from './BoardApplicationCard'
import { DropArea, LaneArea } from './BoardParts'

interface UnassignedColumnProps {
  lane: BoardLane
  busy: boolean
  width: number
}

export function UnassignedColumn({ lane, busy, width }: UnassignedColumnProps): ReactNode {
  return (
    <Stack gap="xs" w={width} flex="none" component="section" aria-label={lane.title}>
      <Text size="sm" fw={600}>
        {lane.title}
      </Text>
      <LaneArea lane={lane} busy={busy} />
    </Stack>
  )
}

export function ApplicationsColumn({ board, busy }: { board: ApplicationBoard; busy: boolean }): ReactNode {
  const { copy, applications } = board

  return (
    <Stack gap="xs" flex={1} miw={0} component="section" aria-label={copy.applications}>
      <Text size="sm" fw={600}>
        {copy.applications}
      </Text>
      <DropArea zone={board.new_application} busy={busy} />
      {applications.length === 0 && (
        <Text size="sm" c="dimmed">
          {copy.applications_empty}
        </Text>
      )}
      {applications.map((application) => (
        <BoardApplicationCard key={application.id} application={application} copy={copy} busy={busy} />
      ))}
    </Stack>
  )
}
