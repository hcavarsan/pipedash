import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { Button, Grid, Group, Loader, Menu, Paper, Select, Stack, Text } from '@mantine/core'
import { IconAdjustments, IconChartLine, IconDownload, IconFileTypeCsv, IconPhoto } from '@tabler/icons-react'

import { useMetricsEventListeners } from '../../hooks/useMetricsEventListeners'
import { useMetricsFilters } from '../../hooks/useUrlState'
import { useAggregatedMetrics, usePipelineMetricsConfig } from '../../queries/useMetricsQueries'
import { service } from '../../services'
import type { AggregationPeriod, AggregationType, MetricType } from '../../types'
import { StandardModal } from '../common/StandardModal'
import { CompactMetricsCard } from '../metrics/CompactMetricsCard'
import { MetricsConfigModal } from '../metrics/MetricsConfigModal'
import { MetricsDetailPage, MetricsDetailPageRef } from '../metrics/MetricsDetailPage'

interface PipelineMetricsViewProps {
  pipelineId: string
  pipelineName: string
  repository?: string
  refreshTrigger?: number
}

const DATE_RANGE_OPTIONS = [
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' },
  { value: '60d', label: 'Last 60 Days' },
  { value: '90d', label: 'Last 90 Days' },
]

const parseDateRangeToDays = (range: string): number => {
  const value = parseInt(range, 10)

  if (range.endsWith('h')) {
    return value / 24
  }

  return value
}

const getDefaultAggregationType = (metricType: MetricType): AggregationType => {
  switch (metricType) {
    case 'success_rate':
      return 'avg'
    case 'run_duration':
      return 'avg'
    case 'run_frequency':
      return 'sum'
  }
}

const getMetricTitle = (type: MetricType): string => {
  switch (type) {
    case 'run_duration':
      return 'Run Duration'
    case 'success_rate':
      return 'Success Rate'
    case 'run_frequency':
      return 'Run Frequency'
  }
}

export const PipelineMetricsView = ({ pipelineId, pipelineName, repository, refreshTrigger }: PipelineMetricsViewProps) => {
  const { filters, setFilter } = useMetricsFilters()
  const dateRange = filters.dateRange
  const aggregationPeriod = filters.period as AggregationPeriod

  const [configModalOpened, setConfigModalOpened] = useState(false)
  const [showDetailPage, setShowDetailPage] = useState(false)
  const [selectedMetricType, setSelectedMetricType] = useState<MetricType>('run_duration')
  const metricsDetailRef = useRef<MetricsDetailPageRef>(null)

  const pipelineConfig = usePipelineMetricsConfig(pipelineId)

  const { startDate, endDate } = useMemo(() => {
    const end = new Date()
    const start = new Date()

    switch (dateRange) {
      case '24h':
        start.setHours(start.getHours() - 24)
        break
      case '7d':
        start.setDate(start.getDate() - 7)
        break
      case '30d':
        start.setDate(start.getDate() - 30)
        break
      case '60d':
        start.setDate(start.getDate() - 60)
        break
      case '90d':
        start.setDate(start.getDate() - 90)
        break
    }

    return {
      startDate: start.toISOString(),
      endDate: end.toISOString(),
    }
  }, [dateRange])

  const isMetricsEnabled = pipelineConfig.data?.enabled ?? false

  const durationQuery = useAggregatedMetrics({
    metricType: 'run_duration',
    aggregationPeriod,
    aggregationType: getDefaultAggregationType('run_duration'),
    pipelineId,
    startDate,
    endDate,
    enabled: isMetricsEnabled,
  })

  const successRateQuery = useAggregatedMetrics({
    metricType: 'success_rate',
    aggregationPeriod,
    aggregationType: getDefaultAggregationType('success_rate'),
    pipelineId,
    startDate,
    endDate,
    enabled: isMetricsEnabled,
  })

  const frequencyQuery = useAggregatedMetrics({
    metricType: 'run_frequency',
    aggregationPeriod,
    aggregationType: getDefaultAggregationType('run_frequency'),
    pipelineId,
    startDate,
    endDate,
    enabled: isMetricsEnabled,
  })

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0 && isMetricsEnabled) {
      durationQuery.refetch()
      successRateQuery.refetch()
      frequencyQuery.refetch()
    }
  }, [refreshTrigger, isMetricsEnabled, durationQuery, successRateQuery, frequencyQuery])

  const hasTriggeredCollectionRef = useRef<string | null>(null)

  useEffect(() => {
    if (
      pipelineConfig.data?.enabled &&
      !pipelineConfig.isLoading &&
      hasTriggeredCollectionRef.current !== pipelineId
    ) {
      hasTriggeredCollectionRef.current = pipelineId

      service.fetchRunHistory(pipelineId, 1, 50).catch((error) => {
        console.warn('Failed to trigger metrics collection:', error)
      })
    }
  }, [pipelineId, pipelineConfig.data?.enabled, pipelineConfig.isLoading])

  const handleConfigSaved = async () => {
    await pipelineConfig.refetch()

    if (pipelineConfig.data?.enabled) {
      await service.clearRunHistoryCache(pipelineId)
      await service.fetchRunHistory(pipelineId, 1, 50)

      durationQuery.refetch()
      successRateQuery.refetch()
      frequencyQuery.refetch()
    }
  }

  const handleExpandMetric = (metricType: MetricType) => {
    setSelectedMetricType(metricType)
    setShowDetailPage(true)
  }

  const handleBackToDashboard = () => {
    setShowDetailPage(false)
  }

  const handleMetricsRefetch = useCallback(() => {
    durationQuery.refetch()
    successRateQuery.refetch()
    frequencyQuery.refetch()
  }, [durationQuery, successRateQuery, frequencyQuery])

  const handleConfigChanged = useCallback(() => {
    pipelineConfig.refetch().then(() => {
      durationQuery.refetch()
      successRateQuery.refetch()
      frequencyQuery.refetch()
    })
  }, [pipelineConfig, durationQuery, successRateQuery, frequencyQuery])

  useMetricsEventListeners({
    pipelineId,
    enabled: isMetricsEnabled,
    onRefetch: handleMetricsRefetch,
    onConfigChanged: handleConfigChanged,
  })

  const getAvailableDateRanges = () => {
    const retentionDays = pipelineConfig.data?.retention_days || 7

    return DATE_RANGE_OPTIONS.map((option) => {
      const days = parseDateRangeToDays(option.value)
      const isDisabled = days > retentionDays

      return {
        ...option,
        disabled: isDisabled,
      }
    })
  }

  const getAvailableAggregations = () => {
    const AGGREGATION_OPTIONS = [
      { value: 'hourly', label: 'Hourly' },
      { value: 'daily', label: 'Daily' },
      { value: 'weekly', label: 'Weekly' },
      { value: 'monthly', label: 'Monthly' },
    ]

    return AGGREGATION_OPTIONS
  }

  let content: React.ReactNode

  if (pipelineConfig.isLoading) {
    content = (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <IconChartLine size={48} stroke={1.5} style={{ opacity: 0.3 }} />
          <Text size="sm" c="dimmed">Loading metrics configuration...</Text>
        </Stack>
      </Paper>
    )
  } else if (!isMetricsEnabled) {
    content = (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <IconChartLine size={48} stroke={1.5} style={{ opacity: 0.5 }} />
          <Text size="lg" fw={500}>
            Metrics Not Enabled
          </Text>
          <Text size="sm" c="dimmed" ta="center">
            Enable metrics collection to track run duration, success rates, and run frequency over
            time.
          </Text>
          <Button
            onClick={() => setConfigModalOpened(true)}
            leftSection={<IconAdjustments size={16} />}
            variant="light"
            color="blue"
            size="sm"
          >
            Enable Metrics
          </Button>
        </Stack>
      </Paper>
    )
  } else {
    const metricsAreLoading =
      (durationQuery.isPending && isMetricsEnabled) ||
      (successRateQuery.isPending && isMetricsEnabled) ||
      (frequencyQuery.isPending && isMetricsEnabled) ||
      durationQuery.isFetching ||
      successRateQuery.isFetching ||
      frequencyQuery.isFetching

    if (metricsAreLoading) {
      content = (
        <Paper p="xl" withBorder>
          <Stack align="center" gap="md">
            <Loader size="lg" />
            <Text size="sm" c="dimmed">Loading metrics...</Text>
          </Stack>
        </Paper>
      )
    } else {
      const hasData = Boolean(
        (durationQuery.data && durationQuery.data.metrics.length > 0) ||
          (successRateQuery.data && successRateQuery.data.metrics.length > 0) ||
          (frequencyQuery.data && frequencyQuery.data.metrics.length > 0)
      )

      if (!hasData) {
        content = (
          <Paper p="xl" withBorder>
            <Stack align="center" gap="md">
              <IconChartLine size={48} stroke={1.5} style={{ opacity: 0.3 }} />
              <Text size="sm" c="dimmed" ta="center">
                No metrics data available yet. Run your pipeline to generate metrics.
              </Text>
              <Button
                onClick={() => {
                  durationQuery.refetch()
                  successRateQuery.refetch()
                  frequencyQuery.refetch()
                }}
                variant="subtle"
                size="sm"
                loading={
                  durationQuery.isFetching || successRateQuery.isFetching || frequencyQuery.isFetching
                }
              >
                Refresh
              </Button>
            </Stack>
          </Paper>
        )
      } else {
        content = (
          <Stack gap="md">
            <Group justify="space-between" wrap="wrap">
              <Group gap="sm" wrap="wrap">
                <Select
                  label="Time Range"
                  value={dateRange}
                  onChange={(value) => setFilter('dateRange', value || '24h')}
                  data={getAvailableDateRanges()}
                  w={140}
                />
                <Select
                  label="Group By"
                  value={aggregationPeriod}
                  onChange={(value) => setFilter('period', value || 'hourly')}
                  data={getAvailableAggregations()}
                  w={120}
                />
              </Group>
              <Button
                size="xs"
                variant="subtle"
                leftSection={<IconAdjustments size={14} />}
                onClick={() => setConfigModalOpened(true)}
              >
                Configure
              </Button>
            </Group>

            <Grid gutter="md">
              <Grid.Col span={{ base: 12, sm: 12, md: 4 }}>
                <CompactMetricsCard
                  metricType="run_duration"
                  data={durationQuery.data ?? null}
                  loading={durationQuery.isFetching}
                  onExpand={() => handleExpandMetric('run_duration')}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 12, md: 4 }}>
                <CompactMetricsCard
                  metricType="success_rate"
                  data={successRateQuery.data ?? null}
                  loading={successRateQuery.isFetching}
                  onExpand={() => handleExpandMetric('success_rate')}
                />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 12, md: 4 }}>
                <CompactMetricsCard
                  metricType="run_frequency"
                  data={frequencyQuery.data ?? null}
                  loading={frequencyQuery.isFetching}
                  onExpand={() => handleExpandMetric('run_frequency')}
                />
              </Grid.Col>
            </Grid>

            <StandardModal
              opened={showDetailPage}
              onClose={handleBackToDashboard}
              title={getMetricTitle(selectedMetricType)}
              disableAspectRatio
              footer={
                <Group justify="flex-end">
                  <Menu position="top-end" shadow="md">
                    <Menu.Target>
                      <Button
                        variant="light"
                        size="sm"
                        leftSection={<IconDownload size={16} />}
                      >
                        Export
                      </Button>
                    </Menu.Target>
                    <Menu.Dropdown>
                      <Menu.Item
                        leftSection={<IconPhoto size={16} />}
                        onClick={() => metricsDetailRef.current?.exportPNG()}
                      >
                        Export as PNG
                      </Menu.Item>
                      <Menu.Item
                        leftSection={<IconFileTypeCsv size={16} />}
                        onClick={() => metricsDetailRef.current?.exportCSV()}
                      >
                        Export as CSV
                      </Menu.Item>
                    </Menu.Dropdown>
                  </Menu>
                </Group>
              }
            >
              {showDetailPage && (
                <MetricsDetailPage
                  ref={metricsDetailRef}
                  pipelineId={pipelineId}
                  pipelineName={pipelineName}
                  repository={repository}
                  metricType={selectedMetricType}
                  initialDateRange={dateRange}
                  initialAggregation={aggregationPeriod}
                  retentionDays={pipelineConfig.data?.retention_days || 7}
                  refreshTrigger={refreshTrigger}
                />
              )}
            </StandardModal>
          </Stack>
        )
      }
    }
  }

  return (
    <>
      {content}

      <MetricsConfigModal
        opened={configModalOpened}
        onClose={() => setConfigModalOpened(false)}
        onConfigChange={handleConfigSaved}
        pipelineId={pipelineId}
        pipelineName={pipelineName}
      />
    </>
  )
}
