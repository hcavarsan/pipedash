import { Group, Stack, Text, Tooltip, VisuallyHidden } from '@mantine/core'
import type { ReactNode } from 'react'
import { ToneDot } from '../../features/pipelines/components/ToneDot'
import { useNow } from '../../hooks/useNow'
import { useNotificationStore } from '../../stores/notificationStore'
import type { ProviderSummary } from '../../types/providers'
import type { ViewPipeline } from '../../types/views'
import { dataAge } from '../../utils/fetchInfo'
import { formatDateTime } from '../../utils/formatDateTime'
import {
  getPipelineStatusLabel,
  isQuietWatched,
  pipelineStatusKey,
  quietDetail,
} from '../../utils/pipelinePresentation'
import { StatusBadge } from '../atoms/StatusBadge'

export function PipelineStatusBadge({
  pipeline,
  provider,
}: {
  pipeline: ViewPipeline
  provider: ProviderSummary | undefined
}): ReactNode {
  const now = useNow(30_000)
  const quietPipelines = useNotificationStore((state) => state.preferences.quiet_pipelines)
  const quiet = isQuietWatched(pipeline, quietPipelines)
  const age = provider && dataAge(provider, now)
  const lastActivity = `Last activity: ${pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never'}`
  const activity = pipeline.resource_state?.message ?? lastActivity

  return (
    <Tooltip
      multiline
      maw={480}
      label={
        <Stack gap="4xs">
          <Text size="xs">{activity}</Text>
          {quiet && <Text size="xs">{quietDetail(pipeline)}</Text>}
          {age && <Text size="xs">{age.label}</Text>}
          {age && <Text size="xs">{age.detail}</Text>}
        </Stack>
      }
    >
      <Group component="span" gap="3xs" wrap="nowrap" display="inline-flex" data-raised>
        <StatusBadge status={pipelineStatusKey(pipeline)} label={getPipelineStatusLabel(pipeline, quiet)} />
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
