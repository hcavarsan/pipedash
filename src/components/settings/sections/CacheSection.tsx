import { Box, Button, Card, Divider, SimpleGrid, Stack, Text } from '@mantine/core'
import { modals } from '@mantine/modals'

import {
  useCacheStats,
  useClearCache,
  useClearPipelinesCache,
  useClearRunHistoryCache,
  useClearWorkflowParamsCache,
} from '../../../queries/useCacheQueries'

interface CacheSectionProps {
  onRefresh?: () => Promise<void>;
}

export const CacheSection = ({ onRefresh }: CacheSectionProps) => {
  const { data: cacheStats } = useCacheStats()

  const clearAllMutation = useClearCache()
  const clearPipelinesMutation = useClearPipelinesCache()
  const clearRunHistoryMutation = useClearRunHistoryCache()
  const clearWorkflowParamsMutation = useClearWorkflowParamsCache()

  const handleClearCache = (type: 'pipelines' | 'run_history' | 'workflow_params' | 'all') => {
    const labels: Record<string, string> = {
      pipelines: 'pipelines cache',
      run_history: 'run history cache',
      workflow_params: 'workflow parameters cache',
      all: 'all caches',
    }

    modals.openConfirmModal({
      title: `Clear ${labels[type]}?`,
      children: <Text size="sm">Data will be re-fetched when needed.</Text>,
      labels: { confirm: 'Clear', cancel: 'Cancel' },
      confirmProps: { color: 'gray' },
      onConfirm: async () => {
        switch (type) {
          case 'pipelines':
            clearPipelinesMutation.mutate()
            break
          case 'run_history':
            clearRunHistoryMutation.mutate()
            break
          case 'workflow_params':
            clearWorkflowParamsMutation.mutate()
            break
          case 'all':
            clearAllMutation.mutate()
            break
        }

        if (onRefresh) {
          await onRefresh()
        }
      },
    })
  }

  const cacheItems = [
    { key: 'pipelines' as const, label: 'Pipelines', count: cacheStats?.pipelines_count },
    { key: 'run_history' as const, label: 'Run history', count: cacheStats?.run_history_count },
    { key: 'workflow_params' as const, label: 'Workflow parameters', count: cacheStats?.workflow_params_count },
  ]

  return (
    <Box>
      <Text size="lg" fw={600} mb="lg">Cache</Text>

      <Stack gap="md">
        <Card withBorder padding="md" radius="md">
          <Stack gap="md">
            {cacheItems.map((item, index) => (
              <Box key={item.key}>
                {index > 0 && <Divider mb="md" />}
                <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
                  <Stack gap={4}>
                    <Text size="xs" c="dimmed">{item.label}</Text>
                    <Text size="sm">
                      {item.count !== undefined ? `${item.count} items` : 'Loading...'}
                    </Text>
                  </Stack>
                  <Stack gap={4} align="flex-end">
                    <Button
                      size="compact-xs"
                      variant="subtle"
                      color="gray"
                      onClick={() => handleClearCache(item.key)}
                      disabled={
                        clearPipelinesMutation.isPending ||
                        clearRunHistoryMutation.isPending ||
                        clearWorkflowParamsMutation.isPending ||
                        clearAllMutation.isPending
                      }
                    >
                      Clear
                    </Button>
                  </Stack>
                </SimpleGrid>
              </Box>
            ))}

            {cacheStats && cacheStats.metrics_count > 0 && (
              <>
                <Divider />
                <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="lg">
                  <Stack gap={4}>
                    <Text size="xs" c="dimmed">Metrics</Text>
                    <Text size="sm">{cacheStats.metrics_count} items</Text>
                  </Stack>
                </SimpleGrid>
              </>
            )}

            <Divider />
            <Button
              fullWidth
              size="sm"
              variant="light"
              color="gray"
              onClick={() => handleClearCache('all')}
              loading={clearAllMutation.isPending}
            >
              Clear all caches
            </Button>
          </Stack>
        </Card>
      </Stack>
    </Box>
  )
}
