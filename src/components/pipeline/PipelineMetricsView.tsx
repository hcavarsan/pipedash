import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { Button, Grid, Group, Loader, Modal, Paper, Select, Stack, Text } from '@mantine/core'
import { IconAdjustments, IconChartLine } from '@tabler/icons-react'
import { listen } from '@tauri-apps/api/event'

import { useMetrics } from '../../hooks/useMetrics'
import { tauriService } from '../../services/tauri'
import type { AggregatedMetrics, AggregationPeriod, AggregationType, MetricType } from '../../types'
import { CompactMetricsCard } from '../metrics/CompactMetricsCard'
import { MetricsConfigModal } from '../metrics/MetricsConfigModal'
import { MetricsDetailPage } from '../metrics/MetricsDetailPage'

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
  const value = parseInt(range)


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

const fillMissingPeriods = (metrics: AggregatedMetrics): AggregatedMetrics => {
  if (!metrics || metrics.metrics.length === 0) {
return metrics
}

  const result = [...metrics.metrics]
  const period = metrics.aggregation_period

  const getNextPeriod = (timestamp: string): Date => {
    const date = new Date(timestamp)


    switch (period) {
      case 'hourly':
        date.setHours(date.getHours() + 1)
        break
      case 'daily':
        date.setDate(date.getDate() + 1)
        break
      case 'weekly':
        date.setDate(date.getDate() + 7)
        break
      case 'monthly':
        date.setMonth(date.getMonth() + 1)
        break
    }

return date
  }

  const filled: typeof metrics.metrics = []


  for (let i = 0; i < result.length - 1; i++) {
    filled.push(result[i])

    let current = new Date(result[i].timestamp)
    const next = new Date(result[i + 1].timestamp)

    while (true) {
      const nextPeriod = getNextPeriod(current.toISOString())


      if (nextPeriod >= next) {
break
}

      filled.push({
        timestamp: nextPeriod.toISOString(),
        value: 0,
        count: 0,
        min: null,
        max: null,
        avg: 0,
      })
      current = nextPeriod
    }
  }
  filled.push(result[result.length - 1])

  return {
    ...metrics,
    metrics: filled,
    total_count: filled.length,
  }
}

type LoadState = 'loading-config' | 'disabled' | 'loading-data' | 'empty' | 'ready'

export const PipelineMetricsView = ({ pipelineId, pipelineName, repository, refreshTrigger }: PipelineMetricsViewProps) => {
  const isMountedRef = useRef(true)
  const { metricsLoading, getPipelineConfig, queryAggregatedMetrics } = useMetrics()
  const [configModalOpened, setConfigModalOpened] = useState(false)
  const [showDetailPage, setShowDetailPage] = useState(false)
  const [selectedMetricType, setSelectedMetricType] = useState<MetricType>('run_duration')
  const [loadState, setLoadState] = useState<LoadState>('loading-config')
  const [retentionDays, setRetentionDays] = useState(7)
  const [aggregationPeriod, setAggregationPeriod] = useState<AggregationPeriod>('hourly')
  const [dateRange, setDateRange] = useState('24h')
  const [durationData, setDurationData] = useState<AggregatedMetrics | null>(null)
  const [successRateData, setSuccessRateData] = useState<AggregatedMetrics | null>(null)
  const [frequencyData, setFrequencyData] = useState<AggregatedMetrics | null>(null)

  useEffect(() => {
    isMountedRef.current = true

    return () => {
      isMountedRef.current = false
    }
  }, [])

  const checkConfig = useCallback(async () => {
    const config = await getPipelineConfig(pipelineId)

    if (isMountedRef.current) {
      setRetentionDays(config?.retention_days || 7)

      if (config?.enabled) {
        setLoadState('loading-data')
      } else {
        setLoadState('disabled')
      }
    }

    return config
  }, [pipelineId, getPipelineConfig])

  const loadAllMetrics = useCallback(async () => {
    if (loadState === 'disabled') {
      return
    }

    const endDate = new Date()
    const startDate = new Date()

    switch (dateRange) {
      case '24h':
        startDate.setHours(startDate.getHours() - 24)
        break
      case '7d':
        startDate.setDate(startDate.getDate() - 7)
        break
      case '30d':
        startDate.setDate(startDate.getDate() - 30)
        break
      case '90d':
        startDate.setDate(startDate.getDate() - 90)
        break
    }

    const [duration, successRate, frequency] = await Promise.all([
      queryAggregatedMetrics(
        'run_duration',
        aggregationPeriod,
        getDefaultAggregationType('run_duration'),
        pipelineId,
        startDate.toISOString(),
        endDate.toISOString()
      ),
      queryAggregatedMetrics(
        'success_rate',
        aggregationPeriod,
        getDefaultAggregationType('success_rate'),
        pipelineId,
        startDate.toISOString(),
        endDate.toISOString()
      ),
      queryAggregatedMetrics(
        'run_frequency',
        aggregationPeriod,
        getDefaultAggregationType('run_frequency'),
        pipelineId,
        startDate.toISOString(),
        endDate.toISOString()
      ),
    ])

    if (!isMountedRef.current) {
return
}

    setDurationData(duration)
    setSuccessRateData(successRate)
    setFrequencyData(frequency)

    const hasData = Boolean(
      (duration && duration.metrics.length > 0) ||
      (successRate && successRate.metrics.length > 0) ||
      (frequency && frequency.metrics.length > 0)
    )

    setLoadState(hasData ? 'ready' : 'empty')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pipelineId, dateRange, aggregationPeriod, queryAggregatedMetrics])

  const handleConfigSaved = useCallback(async () => {
    setLoadState('loading-config')
    const config = await checkConfig()

    if (!isMountedRef.current) {
return
}

    setDurationData(null)
    setSuccessRateData(null)
    setFrequencyData(null)

    if (config?.enabled) {
      setLoadState('loading-data')

      await tauriService.clearRunHistoryCache(pipelineId)
      await tauriService.fetchRunHistory(pipelineId, 1, 50)

      if (isMountedRef.current) {
        await loadAllMetrics()
      }
    } else {
      setLoadState('disabled')
    }

  }, [checkConfig, pipelineId, loadAllMetrics])

  const handleExpandMetric = (metricType: MetricType) => {
    setSelectedMetricType(metricType)
    setShowDetailPage(true)
  }

  const handleBackToDashboard = () => {
    setShowDetailPage(false)
  }

  useEffect(() => {
    if (!isMountedRef.current) {
return
}

    setLoadState('loading-config')
    checkConfig().then((config) => {
      if (isMountedRef.current && config?.enabled) {
        loadAllMetrics()
      }
    })
  }, [pipelineId, checkConfig, loadAllMetrics])

  useEffect(() => {
    if (loadState === 'ready' && isMountedRef.current) {
      loadAllMetrics()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aggregationPeriod, dateRange])

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0 && loadState !== 'disabled' && isMountedRef.current) {
      loadAllMetrics()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshTrigger, loadState])

  useEffect(() => {
    if (loadState === 'disabled' || !pipelineId) {
      return
    }

    let isActive = true
    const unlisteners: Array<() => void> = []

    const setupListeners = async () => {
      try {
        const unlisten1 = await listen<string>('metrics-generated', (event) => {
          if (event.payload === pipelineId && isActive && isMountedRef.current) {
            loadAllMetrics()
          }
        })


        if (!isActive) {
          unlisten1()

return
        }
        unlisteners.push(unlisten1)

        const unlisten2 = await listen<string>('metrics-config-changed', (event) => {
          if (event.payload === pipelineId && isActive && isMountedRef.current) {
            checkConfig().then(() => {
              if (isActive && isMountedRef.current) {
                loadAllMetrics()
              }
            })
          }
        })


        if (!isActive) {
          unlisten2()

return
        }
        unlisteners.push(unlisten2)

        const unlisten3 = await listen<string>('run-triggered', (event) => {
          if (event.payload === pipelineId && isActive && isMountedRef.current) {
            loadAllMetrics()
          }
        })


        if (!isActive) {
          unlisten3()

return
        }
        unlisteners.push(unlisten3)

        const unlisten4 = await listen<string>('run-cancelled', (event) => {
          if (event.payload === pipelineId && isActive && isMountedRef.current) {
            loadAllMetrics()
          }
        })


        if (!isActive) {
          unlisten4()

return
        }
        unlisteners.push(unlisten4)

        const unlisten5 = await listen<any>('pipeline-status-changed', () => {
          if (isActive && isMountedRef.current) {
            loadAllMetrics()
          }
        })


        if (!isActive) {
          unlisten5()

return
        }
        unlisteners.push(unlisten5)
      } catch (error) {
        console.error('Failed to setup metrics event listeners:', error)
      }
    }

    setupListeners()

    return () => {
      isActive = false
      unlisteners.forEach((unlisten) => {
        try {
          unlisten()
        } catch {
        }
      })
    }
  }, [loadState, pipelineId, loadAllMetrics, checkConfig])

  const getAvailableDateRanges = () => {
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

  const filledDurationData = useMemo(() => {
    return durationData ? fillMissingPeriods(durationData) : null
  }, [durationData])

  const filledSuccessRateData = useMemo(() => {
    return successRateData ? fillMissingPeriods(successRateData) : null
  }, [successRateData])

  const filledFrequencyData = useMemo(() => {
    return frequencyData ? fillMissingPeriods(frequencyData) : null
  }, [frequencyData])

  let content: React.ReactNode

  if (loadState === 'loading-config') {
    content = (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <IconChartLine size={48} stroke={1.5} style={{ opacity: 0.3 }} />
          <Text size="sm" c="dimmed">Loading metrics configuration...</Text>
        </Stack>
      </Paper>
    )
  } else if (loadState === 'disabled') {
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
  } else if (loadState === 'loading-data') {
    content = (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <Loader size="lg" />
          <Text size="sm" c="dimmed">Loading metrics...</Text>
        </Stack>
      </Paper>
    )
  } else if (loadState === 'empty') {
    content = (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <IconChartLine size={48} stroke={1.5} style={{ opacity: 0.3 }} />
          <Text size="sm" c="dimmed" ta="center">
            No metrics data available yet. Run your pipeline to generate metrics.
          </Text>
          <Button
            onClick={loadAllMetrics}
            variant="subtle"
            size="sm"
            loading={metricsLoading}
          >
            Refresh
          </Button>
        </Stack>
      </Paper>
    )
  } else {
    // Main metrics view
    content = (
      <Stack gap="md">
        <Group justify="space-between" wrap="wrap">
          <Group gap="sm" wrap="wrap">
            <Select
              label="Time Range"
              value={dateRange}
              onChange={(value) => setDateRange(value || '7d')}
              data={getAvailableDateRanges()}
              w={140}
            />
            <Select
              label="Group By"
              value={aggregationPeriod}
              onChange={(value) => setAggregationPeriod((value as AggregationPeriod) || 'daily')}
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
              data={filledDurationData}
              loading={metricsLoading}
              onExpand={() => handleExpandMetric('run_duration')}
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 12, md: 4 }}>
            <CompactMetricsCard
              metricType="success_rate"
              data={filledSuccessRateData}
              loading={metricsLoading}
              onExpand={() => handleExpandMetric('success_rate')}
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 12, md: 4 }}>
            <CompactMetricsCard
              metricType="run_frequency"
              data={filledFrequencyData}
              loading={metricsLoading}
              onExpand={() => handleExpandMetric('run_frequency')}
            />
          </Grid.Col>
        </Grid>

        <Modal
          opened={showDetailPage}
          onClose={handleBackToDashboard}
          size="98%"
          padding="xl"
          withCloseButton={false}
          centered
          overlayProps={{
            opacity: 0.55,
            blur: 3,
          }}
          styles={{
            body: {
              maxHeight: '95vh',
              overflow: 'auto',
            },
            content: {
              maxHeight: '95vh',
            },
          }}
        >
          {showDetailPage && (
            <MetricsDetailPage
              pipelineId={pipelineId}
              pipelineName={pipelineName}
              repository={repository}
              metricType={selectedMetricType}
              initialDateRange={dateRange}
              initialAggregation={aggregationPeriod}
              retentionDays={retentionDays}
              refreshTrigger={refreshTrigger}
              onBack={handleBackToDashboard}
            />
          )}
        </Modal>
      </Stack>
    )
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
