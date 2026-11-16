import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { useDeployChanges } from '../../applications/queries/useApplicationQueries'

interface DeployChangesSectionProps {
  pipelineId: string
  runNumber: number
}

export function DeployChangesSection({ pipelineId, runNumber }: DeployChangesSectionProps): ReactNode {
  const { data } = useDeployChanges(pipelineId, runNumber)

  if (!data) {
    return null
  }

  return (
    <Stack gap="xs">
      <Text size="sm" fw={600}>
        {data.title}
      </Text>
      {data.unavailable_reason ? (
        <Text size="sm" c="dimmed">
          {data.unavailable_reason}
        </Text>
      ) : (
        data.commits.map((commit) => (
          <Text key={commit.sha} size="sm">
            <Text component="span" ff="monospace" c="dimmed">
              {commit.short}
            </Text>{' '}
            {commit.message}
          </Text>
        ))
      )}
    </Stack>
  )
}
