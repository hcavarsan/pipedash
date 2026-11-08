import { Group, Stack, Text, Tooltip, VisuallyHidden } from '@mantine/core'
import type { ReactNode } from 'react'
import { ToneDot } from '../../features/pipelines/components/ToneDot'
import { useNow } from '../../hooks/useNow'
import type { ProviderSummary } from '../../types/providers'
import type { ViewPipeline } from '../../types/views'
import { dataAge } from '../../utils/fetchInfo'
import { formatDateTime } from '../../utils/formatDateTime'
import { getPipelineStatusLabel, pipelineStatusKey } from '../../utils/pipelinePresentation'
import { StatusBadge } from '../atoms/StatusBadge'

export function PipelineStatusBadge({
  pipeline,
  provider,
}: {
  pipeline: ViewPipeline
  provider: ProviderSummary | undefined
}): ReactNode {
  const now = useNow(30_000)
  const age = provider && dataAge(provider, now)
  const lastActivity = `Last activity: ${pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never'}`
  const activity = pipeline.resource_state?.message ?? lastActivity

  return (
    <Tooltip
      multiline
      maw={480}
      label={
        <Stack gap={2}>
          <Text size="xs">{activity}</Text>
          {age && <Text size="xs">{age.label}</Text>}
          {age && <Text size="xs">{age.detail}</Text>}
        </Stack>
      }
    >
      <Group component="span" gap={4} wrap="nowrap" display="inline-flex" data-raised>
        <StatusBadge status={pipelineStatusKey(pipeline)} label={getPipelineStatusLabel(pipeline)} />
        {age && age.tone !== 'fresh' && (
          <>
            <ToneDot tone={age.tone} />
            <VisuallyHidden>{age.label}</VisuallyHidden>
          </>
        )}
      </Group>
    </Tooltip>
  )
}
