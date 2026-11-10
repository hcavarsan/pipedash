import { Text } from '@mantine/core'
import type { ReactNode } from 'react'
import type { ProviderSummary } from '../../../types/providers'
import type { ViewPipeline } from '../../../types/views'
import { formatDateTime } from '../../../utils/formatDateTime'

interface PipelineCardMetaProps {
  pipeline: ViewPipeline
  provider: ProviderSummary | undefined
}

export function PipelineCardMeta({ pipeline, provider }: PipelineCardMetaProps): ReactNode {
  const organization = pipeline.repository.includes(pipeline.organization) ? null : pipeline.organization
  const parts = [
    provider?.name ?? 'Unknown',
    organization,
    pipeline.repository,
    pipeline.branch,
    pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never run',
  ].filter(Boolean)

  return (
    <Text size="xs" c="dimmed" lineClamp={2}>
      {parts.join(' · ')}
    </Text>
  )
}
