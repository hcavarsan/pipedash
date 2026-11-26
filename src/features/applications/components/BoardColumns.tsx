import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { BoardApplication, BoardCopy, BoardPipeline } from '../../../types/applicationBoard'
import { BoardApplicationCard } from './BoardApplicationCard'
import { DropArea, PipelineCard } from './BoardParts'

interface UnassignedColumnProps {
  pipelines: BoardPipeline[]
  copy: BoardCopy
  busy: boolean
  width: number
}

export function UnassignedColumn({ pipelines, copy, busy, width }: UnassignedColumnProps): ReactNode {
  return (
    <Stack gap="xs" w={width} flex="none" component="section" aria-label={copy.unassigned}>
      <Text size="sm" fw={600}>
        {copy.unassigned}
      </Text>
      <DropArea
        zone={{ kind: 'unassigned' }}
        label={copy.unassigned}
        disabled={busy}
        hint={pipelines.length === 0 ? copy.unassigned_empty : copy.unassigned_drop}
      >
        {pipelines.map((pipeline) => (
          <PipelineCard
            key={pipeline.pipeline_id}
            pipeline={pipeline}
            applicationId={null}
            stage={null}
            disabled={busy}
          />
        ))}
      </DropArea>
    </Stack>
  )
}

interface ApplicationsColumnProps {
  applications: BoardApplication[]
  copy: BoardCopy
  busy: boolean
  newApplicationLabel: string
}

export function ApplicationsColumn({
  applications,
  copy,
  busy,
  newApplicationLabel,
}: ApplicationsColumnProps): ReactNode {
  return (
    <Stack gap="xs" flex={1} miw={0} component="section" aria-label={copy.applications}>
      <Text size="sm" fw={600}>
        {copy.applications}
      </Text>
      <DropArea
        zone={{ kind: 'new_application' }}
        label={newApplicationLabel}
        disabled={busy}
        hint={copy.new_application_drop}
      />
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
