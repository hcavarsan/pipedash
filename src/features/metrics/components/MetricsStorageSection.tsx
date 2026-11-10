import { Button, Card, Group, Stack, Text } from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { MetricsStats } from '../../../types/metrics'
import { formatBytes } from '../../../utils/formatBytes'
import { formatCount } from '../../../utils/formatCount'
import { formatDateTime } from '../../../utils/formatDateTime'
import type { MetricsConfigScope } from '../lib/metricsConfigScope'
import { useMetricsStorageStats } from '../queries/useMetricsQueries'

interface MetricsStorageSectionProps {
  scope: MetricsConfigScope
  saving: boolean
  flushing: boolean
  onFlush: () => void
}

const StatRow = ({ label, value }: { label: string; value: string }) => (
  <Group justify="space-between" gap="sm" wrap="nowrap">
    <Text size="sm" c="dimmed">
      {label}
    </Text>
    <Text size="sm">{value}</Text>
  </Group>
)

export function MetricsStorageSection({ scope, saving, flushing, onFlush }: MetricsStorageSectionProps): ReactNode {
  const mobileShell = useMobileShell()
  const stats = useMetricsStorageStats()
  const pipelineId = scope.scope === 'pipeline' ? scope.pipelineId : undefined
  const deleteLabel = pipelineId ? 'Delete pipeline metrics' : 'Delete all metrics'

  const renderStorage = (data: MetricsStats): ReactNode => {
    const pipelineStats = pipelineId ? data.by_pipeline.find((p) => p.pipeline_id === pipelineId) : undefined
    const pipelineCount = pipelineStats ? formatCount(pipelineStats.metrics_count, 'metric') : '—'
    const lastCleanup = data.last_cleanup_at ? formatDateTime(data.last_cleanup_at) : '—'

    if (mobileShell) {
      return (
        <IosGroup header="Stored metrics">
          {pipelineId && <IosRow title="This pipeline" value={pipelineCount} />}
          <IosRow
            title="All pipelines"
            subtitle={formatBytes(data.estimated_size_bytes)}
            value={formatCount(data.total_metrics_count, 'metric')}
          />
          <IosRow title="Last cleanup" subtitle={`All pipelines · ${lastCleanup}`} />
        </IosGroup>
      )
    }

    return (
      <Stack gap="xs">
        {pipelineId && <StatRow label="This pipeline" value={pipelineCount} />}
        <StatRow
          label="All pipelines"
          value={`${formatCount(data.total_metrics_count, 'metric')} · ${formatBytes(data.estimated_size_bytes)}`}
        />
        <StatRow label="Last cleanup (all pipelines)" value={lastCleanup} />
      </Stack>
    )
  }

  const storage = (
    <QueryBoundary query={stats} loading="Loading metrics storage usage…" failure="Couldn't load storage usage">
      {renderStorage}
    </QueryBoundary>
  )

  if (mobileShell) {
    return (
      <>
        {storage}

        <IosGroup>
          <IosRow title={deleteLabel} destructive disabled={!stats.data || saving || flushing} onClick={onFlush} />
        </IosGroup>
      </>
    )
  }

  return (
    <Card p="sm">
      <Stack gap="sm">
        <Group justify="space-between">
          <Text size="sm" fw={600}>
            Stored metrics
          </Text>
          <Button
            size="xs"
            variant="subtle"
            color="red"
            leftSection={<IconTrash size={14} aria-hidden />}
            onClick={onFlush}
            loading={flushing}
            disabled={!stats.data || saving}
          >
            {deleteLabel}
          </Button>
        </Group>
        {storage}
      </Stack>
    </Card>
  )
}
