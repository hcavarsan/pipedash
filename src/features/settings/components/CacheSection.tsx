import { Box, Button, Card, Divider, Loader, SimpleGrid, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { AccentText } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { haptic } from '../../../lib/haptics'
import { formatBytes } from '../../../utils/formatBytes'
import { formatCount } from '../../../utils/formatCount'
import {
  useCacheStats,
  useClearCache,
  useClearPipelinesCache,
  useClearRunHistoryCache,
  useClearRunLogsCache,
  useClearWorkflowParamsCache,
} from '../queries/useCacheQueries'

interface CacheSectionProps {
  onRefresh?: (() => Promise<void>) | undefined
}

type CacheType = 'pipelines' | 'run_history' | 'workflow_params' | 'run_logs' | 'all'

const CACHE_NAMES: Record<CacheType, string> = {
  pipelines: 'pipelines cache',
  run_history: 'run history cache',
  workflow_params: 'workflow parameters cache',
  run_logs: 'run logs cache',
  all: 'all caches',
}

interface CacheCountProps {
  label: string
  count: string | number
}

const CacheCount = ({ label, count }: CacheCountProps): ReactNode => (
  <Stack gap="3xs">
    <Text size="xs" c="dimmed">
      {label}
    </Text>
    <Text size="sm">{count}</Text>
  </Stack>
)

const CacheMetricsRow = ({ count }: { count: number }): ReactNode => (
  <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
    <CacheCount label="Metrics" count={`${count} items`} />
  </SimpleGrid>
)

interface CacheStatRowProps {
  label: string
  count: string
  clearLabel: string
  pending: boolean
  disabled: boolean
  onClear: () => void
}

const CacheStatRow = ({ label, count, clearLabel, pending, disabled, onClear }: CacheStatRowProps): ReactNode => (
  <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
    <CacheCount label={label} count={count} />
    <Stack gap="3xs" align="flex-end">
      <Button
        size="compact-xs"
        variant="subtle"
        color="gray"
        aria-label={clearLabel}
        onClick={onClear}
        disabled={disabled}
        loading={pending}
      >
        Clear
      </Button>
    </Stack>
  </SimpleGrid>
)

export const CacheSection = ({ onRefresh }: CacheSectionProps): ReactNode => {
  const mobileShell = useMobileShell()
  const { data: cacheStats, isLoading, error, refetch, isRefetching } = useCacheStats()

  const clearMutations = {
    pipelines: useClearPipelinesCache(),
    run_history: useClearRunHistoryCache(),
    workflow_params: useClearWorkflowParamsCache(),
    run_logs: useClearRunLogsCache(),
    all: useClearCache(),
  }
  const isClearing = Object.values(clearMutations).some((mutation) => mutation.isPending)

  const clearNow = (type: CacheType) =>
    clearMutations[type].mutate(undefined, {
      onSuccess: () => {
        haptic('success')
        void onRefresh?.()
      },
      onError: () => haptic('error'),
    })

  const renderContent = () => {
    if (isLoading) {
      return <LoadingState message="Loading cache statistics..." />
    }

    if (!cacheStats) {
      return (
        <ErrorState
          title="Couldn't load cache statistics"
          error={error}
          onRetry={() => refetch()}
          retrying={isRefetching}
        />
      )
    }

    const cacheItems = [
      { key: 'pipelines' as const, label: 'Pipelines', count: formatCount(cacheStats.pipelines_count, 'item') },
      { key: 'run_history' as const, label: 'Run history', count: formatCount(cacheStats.run_history_count, 'item') },
      {
        key: 'workflow_params' as const,
        label: 'Workflow parameters',
        count: formatCount(cacheStats.workflow_params_count, 'item'),
      },
      { key: 'run_logs' as const, label: 'Run logs', count: formatBytes(cacheStats.run_logs_bytes) },
    ]

    if (mobileShell) {
      const clearing = <Loader size={18} aria-label="Clearing" role="status" />
      const clearRows = cacheItems.map((item) => (
        <IosRow
          key={item.key}
          title={<AccentText>{`Clear ${item.label.toLowerCase()}`}</AccentText>}
          ariaLabel={`Clear ${CACHE_NAMES[item.key]}`}
          disabled={isClearing}
          onClick={() => clearNow(item.key)}
          accessory={clearMutations[item.key].isPending ? clearing : undefined}
        />
      ))

      return (
        <Stack gap="lg" pb="md">
          <IosGroup>
            {cacheItems.map((item) => (
              <IosRow key={item.key} title={item.label} value={item.count} />
            ))}
            {cacheStats.metrics_count > 0 && (
              <IosRow title="Metrics" value={formatCount(cacheStats.metrics_count, 'item')} />
            )}
          </IosGroup>

          <IosGroup footer="Cached data is re-fetched when needed.">{clearRows}</IosGroup>

          <IosGroup>
            <IosRow
              title={<AccentText>Clear all caches</AccentText>}
              disabled={isClearing}
              onClick={() => clearNow('all')}
              accessory={clearMutations.all.isPending ? clearing : undefined}
            />
          </IosGroup>
        </Stack>
      )
    }

    const statRows = cacheItems.map((item, index) => (
      <Box key={item.key}>
        {index > 0 && <Divider mb="md" />}
        <CacheStatRow
          label={item.label}
          count={item.count}
          clearLabel={`Clear ${CACHE_NAMES[item.key]}`}
          pending={clearMutations[item.key].isPending}
          disabled={isClearing}
          onClear={() => clearNow(item.key)}
        />
      </Box>
    ))

    return (
      <Card padding="md">
        <Stack gap="md">
          {statRows}

          {cacheStats.metrics_count > 0 && (
            <>
              <Divider />
              <CacheMetricsRow count={cacheStats.metrics_count} />
            </>
          )}

          <Divider />
          <Button
            fullWidth
            variant="light"
            color="gray"
            onClick={() => clearNow('all')}
            loading={clearMutations.all.isPending}
            disabled={isClearing}
          >
            Clear all caches
          </Button>
        </Stack>
      </Card>
    )
  }

  return <Box>{renderContent()}</Box>
}
