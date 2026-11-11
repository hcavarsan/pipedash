import { Paper, Stack } from '@mantine/core'
import type { ReactNode } from 'react'
import { VirtualList } from '../../../components/common/VirtualList'
import type { ApplicationCopy, ApplicationSummary } from '../../../types/applications'
import { ApplicationName, BuildLine } from './ApplicationRow'
import { TargetChips } from './TargetChips'

interface ApplicationCardsProps {
  applications: ApplicationSummary[]
  copy: ApplicationCopy
}

const CARD_HEIGHT = 132

function ApplicationCard({ application, copy }: { application: ApplicationSummary; copy: ApplicationCopy }): ReactNode {
  return (
    <Paper withBorder radius="sm" p="sm" mb="xs">
      <Stack gap="xs">
        <ApplicationName application={application} copy={copy} />
        <TargetChips chips={application.chips} arrows={application.arrows} />
        {application.build && <BuildLine build={application.build} />}
      </Stack>
    </Paper>
  )
}

export function ApplicationCards({ applications, copy }: ApplicationCardsProps): ReactNode {
  return (
    <VirtualList
      items={applications}
      getKey={(application) => String(application.id)}
      estimateSize={CARD_HEIGHT}
      renderItem={(application) => <ApplicationCard application={application} copy={copy} />}
    />
  )
}
