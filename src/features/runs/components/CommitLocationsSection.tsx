import { Anchor, Badge, Group, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { useCommitLocations } from '../../applications/queries/useApplicationQueries'
import { DeployChangesSection } from './DeployChangesSection'

interface CommitLocationsSectionProps {
  pipelineId: string
  runNumber: number
  failed: boolean
}

export function CommitLocationsSection({ pipelineId, runNumber, failed }: CommitLocationsSectionProps): ReactNode {
  const { data } = useCommitLocations(pipelineId, runNumber)

  if (!data || data.entries.length === 0) {
    return null
  }

  return (
    <Stack gap="md">
      <Stack gap="xs">
        <Text size="sm" fw={600}>
          {data.title}
        </Text>
        {data.entries.map((entry) => (
          <Group key={`${entry.application_id}:${entry.target_label}`} gap="xs" wrap="wrap">
            <Anchor component={Link} to={`/applications/${entry.application_id}`} size="sm">
              {entry.application_name}
            </Anchor>
            <Text size="sm" c="dimmed">
              {entry.target_label}
            </Text>
            <Badge variant="default" size="sm">
              {entry.label}
            </Badge>
          </Group>
        ))}
      </Stack>
      {failed && <DeployChangesSection pipelineId={pipelineId} runNumber={runNumber} />}
    </Stack>
  )
}
