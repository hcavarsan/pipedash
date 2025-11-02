import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { Badge, Card, Grid, Group, Paper, Skeleton, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconActivity, IconChartLine, IconClock } from '@tabler/icons-react'

import type { AggregatedMetrics, MetricType } from '../../types'
import { formatDuration } from '../../utils/formatDuration'

interface MetricsChartProps {
  data: AggregatedMetrics | null
  loading?: boolean
  compact?: boolean
  selectedIndex?: number | null
  onDataPointClick?: (index: number | null) => void
  chartTitle?: string
  chartRef?: React.RefObject<HTMLDivElement | null>
}

interface CustomTooltipProps {
  label?: string | number
  payload?: any[]
  metricType: MetricType
}

interface CustomDotProps {
  cx?: number
  cy?: number
  payload?: any
  fill?: string
  stroke?: string
  onClick?: (index: number) => void
}

const CustomDot = ({ cx, cy, payload, fill, stroke, onClick }: CustomDotProps) => {
  if (cx === undefined || cy === undefined || !payload) {
return null
}

  const isSelected = payload.isSelected || false
  const index = payload.index

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    if (onClick && index !== undefined) {
      onClick(index)
    }
  }

  if (isSelected) {
    return (
      <g onClick={handleClick} style={{ cursor: 'pointer' }}>
        <circle cx={cx} cy={cy} r={10} fill={fill} fillOpacity={0.2} stroke="none" />
        <circle cx={cx} cy={cy} r={6} fill={fill} stroke={stroke} strokeWidth={3} />
      </g>
    )
  }

  return (
    <circle
      cx={cx}
      cy={cy}
      r={4}
      fill="white"
      stroke={stroke}
      strokeWidth={2}
      onClick={handleClick}
      style={{ cursor: 'pointer' }}
    />
  )
}

const CustomTooltip = ({ label, payload, metricType }: CustomTooltipProps) => {
  if (!payload || payload.length === 0) {
return null
}

  const data = payload[0]?.payload


  if (!data) {
return null
}

  const formatValue = (value: number): string => {
    if (metricType === 'run_duration') {
      return formatDuration(value)
    }
    if (metricType === 'success_rate') {
      return `${value.toFixed(1)}%`
    }
    
return value.toFixed(0)
  }

  return (
    <Paper p="xs" withBorder shadow="md" style={{ minWidth: 150 }}>
      <Stack gap={4}>
        <Text size="xs" fw={600}>
          {String(label)}
        </Text>
        <Text size="sm" fw={500}>
          {formatValue(data.value)}
        </Text>
      </Stack>
    </Paper>
  )
}

export const MetricsChart = ({ data, loading, compact = false, selectedIndex, onDataPointClick, chartTitle, chartRef }: MetricsChartProps) => {
  const chartHeight = compact ? 150 : 450

  if (loading) {
    return (
      <Stack gap="md">
        {!compact && (
          <Grid gutter="md">
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <Card p="md" withBorder>
                <Skeleton height={80} />
              </Card>
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <Card p="md" withBorder>
                <Skeleton height={80} />
              </Card>
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <Card p="md" withBorder>
                <Skeleton height={80} />
              </Card>
            </Grid.Col>
          </Grid>
        )}
        <Paper p="md" withBorder>
          <Skeleton height={chartHeight} />
        </Paper>
      </Stack>
    )
  }

  if (!data || data.metrics.length === 0) {
    return (
      <Paper p="xl" withBorder>
        <Stack align="center" gap="md">
          <ThemeIcon size={compact ? 48 : 64} radius="xl" variant="light" color="gray">
            <IconChartLine size={compact ? 24 : 32} />
          </ThemeIcon>
          <Text size={compact ? 'sm' : 'lg'} fw={500} c="dimmed">
            No metrics data available
          </Text>
          {!compact && (
            <Text size="sm" c="dimmed" ta="center">
              Metrics will appear here once pipeline runs are collected during the selected period
            </Text>
          )}
        </Stack>
      </Paper>
    )
  }

  const formatChartDate = (timestamp: string, aggregationPeriod: string) => {
    const date = new Date(timestamp)

    switch (aggregationPeriod) {
      case 'hourly':
        return date.toLocaleString('en-US', {
          month: 'short',
          day: 'numeric',
          hour: '2-digit',
          minute: '2-digit'
        })
      case 'daily':
        return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
      case 'weekly':
        return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
      case 'monthly':
        return date.toLocaleDateString('en-US', { month: 'short', year: 'numeric' })
      default:
        return date.toLocaleString()
    }
  }

  const chartData = data.metrics.map((m, index) => ({
    date: formatChartDate(m.timestamp, data.aggregation_period),
    value: parseFloat(m.value.toFixed(2)),
    count: m.count,
    index,
    timestamp: m.timestamp,
    isSelected: selectedIndex === index,
  }))

  const handleDotClick = (index: number) => {
    if (!onDataPointClick) {
return
}
    if (selectedIndex === index) {
      onDataPointClick(null)
    } else {
      onDataPointClick(index)
    }
  }

  const getChartConfig = (metricType: MetricType) => {
    switch (metricType) {
      case 'run_duration':
        return {
          title: 'Pipeline Run Duration',
          valueLabel: 'Duration',
          color: 'blue',
          icon: IconClock,
        }
      case 'success_rate':
        return {
          title: 'Success Rate',
          valueLabel: 'Success Rate (%)',
          color: 'green',
          icon: IconActivity,
        }
      case 'run_frequency':
        return {
          title: 'Run Frequency',
          valueLabel: 'Number of Runs',
          color: 'violet',
          icon: IconActivity,
        }
    }
  }

  const config = getChartConfig(data.metric_type)

  const calculateStats = () => {
    const realValues = data.metrics.filter((m) => m.count > 0).map((m) => m.value)


    if (realValues.length === 0) {
      return { avg: 0, max: 0, min: 0, latest: 0, trend: 0, count: 0, totalPeriods: data.metrics.length }
    }
    const sum = realValues.reduce((acc, val) => acc + val, 0)
    const avg = sum / realValues.length
    const max = Math.max(...realValues)
    const min = Math.min(...realValues)
    const latest = realValues[realValues.length - 1]
    const previous = realValues.length > 1 ? realValues[realValues.length - 2] : null
    const trend = previous ? ((latest - previous) / previous) * 100 : 0

    return { avg, max, min, latest, trend, count: realValues.length, totalPeriods: data.metrics.length }
  }

  const stats = calculateStats()

  const formatValue = (value: number, metricType: MetricType): string => {
    if (metricType === 'run_duration') {
      return formatDuration(value)
    }
    if (metricType === 'success_rate') {
      return `${value.toFixed(1)}%`
    }
    
return value.toFixed(0)
  }

  const StatCard = ({ label, value, suffix = '' }: { label: string; value: number; suffix?: string }) => (
    <Card p="sm" withBorder>
      <Stack gap={4}>
        <Text size="xs" c="dimmed" tt="uppercase" fw={600}>
          {label}
        </Text>
        <Text size="lg" fw={700}>
          {data.metric_type === 'run_duration' ? formatValue(value, data.metric_type) : `${value.toFixed(2)}${suffix}`}
        </Text>
      </Stack>
    </Card>
  )

  if (data.metric_type === 'run_duration') {
    const selectedDataPoint = selectedIndex !== null && selectedIndex !== undefined ? chartData[selectedIndex] : null

    return (
      <Stack gap="md">
        {!compact && (
          <Grid gutter="sm">
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Average" value={stats.avg} />
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Latest" value={stats.latest} />
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Max Duration" value={stats.max} />
            </Grid.Col>
          </Grid>
        )}

        <Paper p="md" withBorder ref={chartRef}>
          {!compact && (
            <Group mb="md" justify="space-between">
              <Group gap="xs">
                <ThemeIcon size="lg" radius="md" variant="light" color={config.color}>
                  <config.icon size={20} />
                </ThemeIcon>
                <div>
                  <Text size="md" fw={600}>
                    {chartTitle || config.title}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {stats.totalPeriods} periods{stats.count !== stats.totalPeriods ? ` (${stats.count} with data)` : ''}
                  </Text>
                </div>
              </Group>
              {selectedDataPoint && (
                <Badge size="lg" color={config.color} variant="light">
                  <Group gap={6}>
                    <Text size="xs" fw={500}>
                      {selectedDataPoint.date}:
                    </Text>
                    <Text size="xs" fw={700}>
                      {formatValue(selectedDataPoint.value, data.metric_type)}
                    </Text>
                  </Group>
                </Badge>
              )}
            </Group>
          )}
          <ResponsiveContainer width="100%" height={chartHeight}>
            <LineChart data={chartData} margin={{ top: 5, right: 5, left: 5, bottom: 5 }}>
              {!compact && (
                <CartesianGrid
                  strokeDasharray="3 3"
                  stroke="var(--mantine-color-gray-2)"
                  vertical={false}
                />
              )}
              {!compact && (
                <XAxis
                  dataKey="date"
                  angle={-45}
                  textAnchor="end"
                  height={80}
                  axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
                />
              )}
              {!compact && (
                <YAxis
                  tickFormatter={(value) => formatValue(value, data.metric_type)}
                  width={80}
                  axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
                />
              )}
              {!compact && (
                <Tooltip
                  content={({ label, payload }) => (
                    <CustomTooltip label={label} payload={payload} metricType={data.metric_type} />
                  )}
                  cursor={false}
                  isAnimationActive={false}
                  animationDuration={0}
                  wrapperStyle={{ pointerEvents: 'none', outline: 'none' }}
                />
              )}
              <Line
                type="monotone"
                dataKey="value"
                stroke={`var(--mantine-color-${config.color}-6)`}
                strokeWidth={2.5}
                fill={`var(--mantine-color-${config.color}-6)`}
                dot={
                  !compact
                    ? (props: any) => <CustomDot {...props} onClick={handleDotClick} />
                    : false
                }
                activeDot={false}
                isAnimationActive={false}
              />
            </LineChart>
          </ResponsiveContainer>
        </Paper>
      </Stack>
    )
  }

  if (data.metric_type === 'success_rate') {
    const selectedDataPoint = selectedIndex !== null && selectedIndex !== undefined ? chartData[selectedIndex] : null

    return (
      <Stack gap="md">
        {!compact && (
          <Grid gutter="sm">
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Average" value={stats.avg} suffix="%" />
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Latest" value={stats.latest} suffix="%" />
            </Grid.Col>
            <Grid.Col span={{ base: 12, sm: 4 }}>
              <StatCard label="Peak Rate" value={stats.max} suffix="%" />
            </Grid.Col>
          </Grid>
        )}

        <Paper p="md" withBorder ref={chartRef}>
          {!compact && (
            <Group mb="md" justify="space-between">
              <Group gap="xs">
                <ThemeIcon size="lg" radius="md" variant="light" color={config.color}>
                  <config.icon size={20} />
                </ThemeIcon>
                <div>
                  <Text size="md" fw={600}>
                    {chartTitle || config.title}
                  </Text>
                  <Text size="xs" c="dimmed">
                    {stats.totalPeriods} periods{stats.count !== stats.totalPeriods ? ` (${stats.count} with data)` : ''}
                  </Text>
                </div>
              </Group>
              {selectedDataPoint && (
                <Badge size="lg" color={config.color} variant="light">
                  <Group gap={6}>
                    <Text size="xs" fw={500}>
                      {selectedDataPoint.date}:
                    </Text>
                    <Text size="xs" fw={700}>
                      {formatValue(selectedDataPoint.value, data.metric_type)}
                    </Text>
                  </Group>
                </Badge>
              )}
            </Group>
          )}
          <ResponsiveContainer width="100%" height={chartHeight}>
            <AreaChart data={chartData} margin={{ top: 5, right: 5, left: 5, bottom: 5 }}>
              {!compact && (
                <CartesianGrid
                  strokeDasharray="3 3"
                  stroke="var(--mantine-color-gray-2)"
                  vertical={false}
                />
              )}
              {!compact && (
                <XAxis
                  dataKey="date"
                  angle={-45}
                  textAnchor="end"
                  height={80}
                  axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
                />
              )}
              {!compact && (
                <YAxis
                  tickFormatter={(value) => `${value}%`}
                  width={80}
                  axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                  tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
                />
              )}
              {!compact && (
                <Tooltip
                  content={({ label, payload }) => (
                    <CustomTooltip label={label} payload={payload} metricType={data.metric_type} />
                  )}
                  cursor={false}
                  isAnimationActive={false}
                  animationDuration={0}
                  wrapperStyle={{ pointerEvents: 'none', outline: 'none' }}
                />
              )}
              <Area
                type="monotone"
                dataKey="value"
                stroke={`var(--mantine-color-${config.color}-6)`}
                fill={`var(--mantine-color-${config.color}-6)`}
                fillOpacity={0.3}
                strokeWidth={2.5}
                dot={
                  !compact
                    ? (props: any) => <CustomDot {...props} onClick={handleDotClick} />
                    : false
                }
                activeDot={false}
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        </Paper>
      </Stack>
    )
  }

  const selectedDataPoint = selectedIndex !== null && selectedIndex !== undefined ? chartData[selectedIndex] : null

  return (
    <Stack gap="md">
      {!compact && (
        <Grid gutter="sm">
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <StatCard label="Average" value={stats.avg} suffix=" runs" />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <StatCard label="Latest" value={stats.latest} suffix=" runs" />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <StatCard label="Peak Activity" value={stats.max} suffix=" runs" />
          </Grid.Col>
        </Grid>
      )}

      <Paper p="md" withBorder>
        {!compact && (
          <Group mb="md" justify="space-between">
            <Group gap="xs">
              <ThemeIcon size="lg" radius="md" variant="light" color={config.color}>
                <config.icon size={20} />
              </ThemeIcon>
              <div>
                <Text size="md" fw={600}>
                  {chartTitle || config.title}
                </Text>
                <Text size="xs" c="dimmed">
                  {stats.count} data points
                </Text>
              </div>
            </Group>
            {selectedDataPoint && (
              <Badge size="lg" color={config.color} variant="light">
                <Group gap={6}>
                  <Text size="xs" fw={500}>
                    {selectedDataPoint.date}:
                  </Text>
                  <Text size="xs" fw={700}>
                    {formatValue(selectedDataPoint.value, data.metric_type)}
                  </Text>
                </Group>
              </Badge>
            )}
          </Group>
        )}
        <ResponsiveContainer width="100%" height={chartHeight}>
          <BarChart data={chartData} margin={{ top: 5, right: 5, left: 5, bottom: 5 }}>
            {!compact && (
              <CartesianGrid
                strokeDasharray="3 3"
                stroke="var(--mantine-color-gray-2)"
                vertical={false}
              />
            )}
            {!compact && (
              <XAxis
                dataKey="date"
                angle={-45}
                textAnchor="end"
                height={80}
                axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
              />
            )}
            {!compact && (
              <YAxis
                tickFormatter={(value) => value.toFixed(0)}
                width={80}
                axisLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                tickLine={{ stroke: 'var(--mantine-color-gray-4)' }}
                tick={{ fill: 'var(--mantine-color-gray-6)', fontSize: 11 }}
              />
            )}
            {!compact && (
              <Tooltip
                content={({ label, payload }) => (
                  <CustomTooltip label={label} payload={payload} metricType={data.metric_type} />
                )}
                cursor={false}
              />
            )}
            <Bar
              dataKey="value"
              fill={`var(--mantine-color-${config.color}-6)`}
              radius={[4, 4, 0, 0]}
              onClick={(data: any) => {
                if (!compact && data && data.index !== undefined) {
                  handleDotClick(data.index)
                }
              }}
              shape={(props: any) => {
                const { fill, x, y, width, height, payload } = props
                const isSelected = payload?.isSelected || false


                
return (
                  <rect
                    x={x}
                    y={y}
                    width={width}
                    height={height}
                    fill={fill}
                    fillOpacity={isSelected ? 1 : 0.8}
                    stroke={isSelected ? `var(--mantine-color-${config.color}-8)` : 'none'}
                    strokeWidth={isSelected ? 2 : 0}
                    rx={4}
                    ry={4}
                    style={{ cursor: !compact && onDataPointClick ? 'pointer' : 'default' }}
                  />
                )
              }}
              isAnimationActive={false}
            />
          </BarChart>
        </ResponsiveContainer>
      </Paper>
    </Stack>
  )
}
