import { Button, Card, Group, Skeleton, Stack, Text } from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
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
  const pipelineStats = pipelineId ? stats.data?.by_pipeline.find((p) => p.pipeline_id === pipelineId) : undefined
  const deleteLabel = pipelineId ? 'Delete pipeline metrics' : 'Delete all metrics'

  const renderStorage = () => {
    if (stats.isError) {
      return (
        <ErrorState
          title="Couldn't load storage usage"
          error={stats.error}
          onRetry={() => stats.refetch()}
          retrying={stats.isFetching}
        />
      )
    }

    if (!stats.data) {
      return mobileShell ? <Skeleton height={64} /> : <LoadingState message="Loading metrics storage usage…" />
    }

    return (
      <Stack gap="xs">
        {pipelineId && (
          <StatRow
            label="This pipeline"
            value={pipelineStats ? formatCount(pipelineStats.metrics_count, 'metric') : '—'}
          />
        )}
        <StatRow
          label="All pipelines"
          value={`${formatCount(stats.data.total_metrics_count, 'metric')} · ${stats.data.estimated_size_mb.toFixed(2)} MB`}
        />
        <StatRow
          label="Last cleanup (all pipelines)"
          value={stats.data.last_cleanup_at ? formatDateTime(stats.data.last_cleanup_at) : '—'}
        />
      </Stack>
    )
  }

  if (mobileShell) {
    return (
      <>
        {stats.isError || !stats.data ? (
          renderStorage()
        ) : (
          <IosGroup header="Stored metrics">
            {pipelineId && (
              <IosRow title="This pipeline" value={formatCount(pipelineStats?.metrics_count ?? 0, 'metric')} />
            )}
            <IosRow
              title="All pipelines"
              subtitle={`${stats.data.estimated_size_mb.toFixed(2)} MB`}
              value={formatCount(stats.data.total_metrics_count, 'metric')}
            />
            <IosRow
              title="Last cleanup"
              subtitle={`All pipelines · ${stats.data.last_cleanup_at ? formatDateTime(stats.data.last_cleanup_at) : '—'}`}
            />
          </IosGroup>
        )}

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
        {renderStorage()}
      </Stack>
    </Card>
  )
}
