import { Button, Group } from '@mantine/core'
import { IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { usePluginPresentation } from '../../../queries/usePluginsQueries'
import { useRerunJob } from '../../../queries/useRunHistoryQueries'
import type { RunLogNode } from '../../../types/logs'
import { useRunDetails } from '../queries/useRunDetailsQuery'

interface RerunJobBarProps {
  pipelineId: string
  runNumber: number
  providerType: string
  node: RunLogNode
}

export const RerunJobBar = ({ pipelineId, runNumber, providerType, node }: RerunJobBarProps): ReactNode => {
  const { data: run } = useRunDetails(pipelineId, runNumber)
  const presentation = usePluginPresentation(providerType)
  const rerunJob = useRerunJob()

  if (!run?.actions.rerun_job.allowed || !presentation) {
    return null
  }

  return (
    <Group justify="flex-end" px="sm" py="xs" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
      <Button
        variant="default"
        size="xs"
        leftSection={<IconRefresh size={14} aria-hidden />}
        loading={rerunJob.isPending}
        onClick={() => {
          rerunJob
            .mutateAsync({
              pipelineId,
              runNumber,
              jobId: node.id,
              jobName: node.name,
              confirmation: presentation.rerun_job_confirmation,
            })
            .catch(() => undefined)
        }}
      >
        Re-run job
      </Button>
    </Group>
  )
}
