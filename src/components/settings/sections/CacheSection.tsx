import { Box, Button, Card, Divider, Loader, SimpleGrid, Stack, Text, Title } from '@mantine/core'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { haptic } from '../../../lib/haptics'
import {
  useCacheStats,
  useClearCache,
  useClearPipelinesCache,
  useClearRunHistoryCache,
  useClearWorkflowParamsCache,
} from '../../../queries/useCacheQueries'
import { openConfirm } from '../../../utils/confirm'
import { formatCount } from '../../../utils/formatCount'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { AccentText } from '../mobileParts'

interface CacheSectionProps {
  onRefresh?: () => Promise<void>
}

type CacheType = 'pipelines' | 'run_history' | 'workflow_params' | 'all'

const CACHE_NAMES: Record<CacheType, string> = {
  pipelines: 'pipelines cache',
  run_history: 'run history cache',
  workflow_params: 'workflow parameters cache',
  all: 'all caches',
}

export const CacheSection = ({ onRefresh }: CacheSectionProps) => {
  const mobileShell = useMobileShell()
  const { data: cacheStats, isLoading, error, refetch, isRefetching } = useCacheStats()

  const clearMutations = {
    pipelines: useClearPipelinesCache(),
    run_history: useClearRunHistoryCache(),
    workflow_params: useClearWorkflowParamsCache(),
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

  const handleClearCache = (type: CacheType) => {
    if (mobileShell) {
      clearNow(type)

      return
    }

    openConfirm({
      title: `Clear ${CACHE_NAMES[type]}?`,
      children: <Text size="sm">Cached data will be deleted and re-fetched when needed.</Text>,
      labels: { confirm: `Clear ${CACHE_NAMES[type]}`, cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        clearMutations[type].mutate()

        if (onRefresh) {
          await onRefresh()
        }
      },
    })
  }

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
      { key: 'pipelines' as const, label: 'Pipelines', count: cacheStats.pipelines_count },
      { key: 'run_history' as const, label: 'Run history', count: cacheStats.run_history_count },
      { key: 'workflow_params' as const, label: 'Workflow parameters', count: cacheStats.workflow_params_count },
    ]

    if (mobileShell) {
      const clearing = <Loader size={18} aria-label="Clearing" role="status" />

      return (
        <Stack gap="lg" pb="md">
          <IosGroup>
            {cacheItems.map((item) => (
              <IosRow key={item.key} title={item.label} value={formatCount(item.count, 'item')} />
            ))}
            {cacheStats.metrics_count > 0 && (
              <IosRow title="Metrics" value={formatCount(cacheStats.metrics_count, 'item')} />
            )}
          </IosGroup>

          <IosGroup footer="Cached data is re-fetched when needed.">
            {cacheItems.map((item) => (
              <IosRow
                key={item.key}
                title={<AccentText>{`Clear ${item.label.toLowerCase()}`}</AccentText>}
                ariaLabel={`Clear ${CACHE_NAMES[item.key]}`}
                disabled={isClearing}
                onClick={() => handleClearCache(item.key)}
                accessory={clearMutations[item.key].isPending ? clearing : undefined}
              />
            ))}
          </IosGroup>

          <IosGroup>
            <IosRow
              title={<AccentText>Clear all caches</AccentText>}
              disabled={isClearing}
              onClick={() => handleClearCache('all')}
              accessory={clearMutations.all.isPending ? clearing : undefined}
            />
          </IosGroup>
        </Stack>
      )
    }

    return (
      <Card padding="md">
        <Stack gap="md">
          {cacheItems.map((item, index) => (
            <Box key={item.key}>
              {index > 0 && <Divider mb="md" />}
              <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
                <Stack gap={4}>
                  <Text size="xs" c="dimmed">
                    {item.label}
                  </Text>
                  <Text size="sm">{item.count} items</Text>
                </Stack>
                <Stack gap={4} align="flex-end">
                  <Button
                    size="compact-xs"
                    variant="subtle"
                    color="gray"
                    aria-label={`Clear ${CACHE_NAMES[item.key]}`}
                    onClick={() => handleClearCache(item.key)}
                    disabled={isClearing}
                  >
                    Clear
                  </Button>
                </Stack>
              </SimpleGrid>
            </Box>
          ))}

          {cacheStats.metrics_count > 0 && (
            <>
              <Divider />
              <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
                <Stack gap={4}>
                  <Text size="xs" c="dimmed">
                    Metrics
                  </Text>
                  <Text size="sm">{cacheStats.metrics_count} items</Text>
                </Stack>
              </SimpleGrid>
            </>
          )}

          <Divider />
          <Button
            fullWidth
            variant="light"
            color="gray"
            onClick={() => handleClearCache('all')}
            loading={clearMutations.all.isPending}
          >
            Clear all caches
          </Button>
        </Stack>
      </Card>
    )
  }

  return (
    <Box>
      {!mobileShell && (
        <Title order={2} size="lg" fw={600} mb="lg">
          Cache
        </Title>
      )}

      {renderContent()}
    </Box>
  )
}
