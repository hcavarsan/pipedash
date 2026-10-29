import { Menu, Paper, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconCheck, IconChevronDown, IconChevronUp } from '@tabler/icons-react'
import { type ReactNode, useMemo, useState } from 'react'
import { VirtualList } from '../../../components/common/VirtualList'
import { VirtualTable, type VirtualTableColumn } from '../../../components/common/VirtualTable'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { AggregatedMetric, AggregatedMetrics, MetricType } from '../../../types/domain'
import { formatDateTime } from '../../../utils/formatDateTime'
import { isOneOf } from '../../../utils/isOneOf'
import { formatMetricValue, formatNumber, hasObservation, METRIC_CONFIG } from '../lib/metricsUtils'
import classes from './metrics.module.css'

interface MetricsDataTableProps {
  data: AggregatedMetrics
  selectedIndex: number | null
  onRowClick: (index: number | null) => void
}

const SORT_FIELDS = ['timestamp', 'value', 'count', 'min', 'max'] as const

type SortField = (typeof SORT_FIELDS)[number]
type SortDirection = 'asc' | 'desc'

interface IndexedMetric {
  metric: AggregatedMetric
  originalIndex: number
}

const ROW_HEIGHT = 44
const TABLE_HEIGHT = 480
const SHELL_ROW_HEIGHT = 64

const getKey = ({ originalIndex }: IndexedMetric): string => String(originalIndex)

const getSortValue = (metric: AggregatedMetric, field: SortField, metricType: MetricType): number | null => {
  switch (field) {
    case 'timestamp':
      return new Date(metric.timestamp).getTime()
    case 'value':
      return hasObservation(metric, metricType) ? metric.value : null
    case 'count':
      return metric.count
    case 'min':
      return metric.min
    case 'max':
      return metric.max
  }
}

export const MetricsDataTable = ({ data, selectedIndex, onRowClick }: MetricsDataTableProps): ReactNode => {
  const mobileShell = useMobileShell()
  const [sort, setSort] = useState<{ field: SortField; direction: SortDirection } | null>(null)
  const metricType = data.metric_type

  const columns: { field: SortField; label: string; numeric: boolean }[] = [
    { field: 'timestamp', label: 'Date', numeric: false },
    { field: 'value', label: METRIC_CONFIG[metricType].valueLabel, numeric: true },
    { field: 'count', label: 'Count', numeric: true },
    { field: 'min', label: 'Min', numeric: true },
    { field: 'max', label: 'Max', numeric: true },
  ]

  const handleSort = (field: SortField) => {
    if (sort?.field !== field) {
      setSort({ field, direction: 'asc' })
    } else if (sort.direction === 'asc') {
      setSort({ field, direction: 'desc' })
    } else {
      setSort(null)
    }
  }

  const indexed = useMemo<IndexedMetric[]>(
    () => data.metrics.map((metric, originalIndex) => ({ metric, originalIndex })),
    [data.metrics]
  )

  const sortedMetrics = useMemo(() => {
    if (!sort) {
      return indexed
    }

    return indexed.toSorted((a, b) => {
      const aValue = getSortValue(a.metric, sort.field, metricType)
      const bValue = getSortValue(b.metric, sort.field, metricType)

      if (aValue === null || bValue === null) {
        return Number(aValue === null) - Number(bValue === null)
      }

      return sort.direction === 'asc' ? aValue - bValue : bValue - aValue
    })
  }, [indexed, sort, metricType])

  const select = (originalIndex: number) => onRowClick(selectedIndex === originalIndex ? null : originalIndex)

  if (mobileShell) {
    const sortLabel = sort
      ? `${columns.find(({ field }) => field === sort.field)?.label} · ${sort.direction === 'asc' ? 'Ascending' : 'Descending'}`
      : 'Default'

    return (
      <Stack gap="lg">
        <IosGroup footer="Choose the same column again to reverse or clear the order.">
          <Menu position="bottom" withinPortal>
            <Menu.Target>
              <UnstyledButton className={classes.menuRow}>
                <span>Sort by</span>
                <span className={classes.menuRowValue}>{sortLabel}</span>
              </UnstyledButton>
            </Menu.Target>
            <Menu.Dropdown>
              {columns.map(({ field, label }) => (
                <Menu.Item
                  key={field}
                  onClick={() => handleSort(field)}
                  rightSection={
                    sort?.field === field ? (
                      sort.direction === 'asc' ? (
                        <IconChevronUp size={16} aria-hidden />
                      ) : (
                        <IconChevronDown size={16} aria-hidden />
                      )
                    ) : null
                  }
                >
                  {label}
                </Menu.Item>
              ))}
              <ActionSheetCancel />
            </Menu.Dropdown>
          </Menu>
        </IosGroup>

        <IosGroup header={`Data points (${data.metrics.length} periods)`}>
          <VirtualList
            items={sortedMetrics}
            getKey={getKey}
            estimateSize={SHELL_ROW_HEIGHT}
            renderItem={({ metric, originalIndex }) => {
              const isSelected = selectedIndex === originalIndex
              const range = `Min ${formatMetricValue(metric.min, metricType)} · Max ${formatMetricValue(metric.max, metricType)}`

              return (
                <IosRow
                  title={formatDateTime(metric.timestamp)}
                  subtitle={<span className={classes.num}>{`Count ${formatNumber(metric.count)} · ${range}`}</span>}
                  value={
                    <span className={classes.num}>
                      {formatMetricValue(hasObservation(metric, metricType) ? metric.value : null, metricType)}
                    </span>
                  }
                  accessory={isSelected ? <IconCheck size={18} color="var(--ios-tint)" aria-hidden /> : undefined}
                  selected={isSelected}
                  onClick={() => select(originalIndex)}
                />
              )
            }}
          />
        </IosGroup>
      </Stack>
    )
  }

  const tableColumns: VirtualTableColumn<IndexedMetric>[] = columns.map(({ field, label, numeric }) => ({
    id: field,
    title: label,
    sortable: true,
    textAlign: numeric ? 'right' : 'left',
    render: ({ metric, originalIndex }) => {
      if (field === 'timestamp') {
        const isSelected = selectedIndex === originalIndex

        return (
          <UnstyledButton
            className="item-primary"
            fz="sm"
            fw={isSelected ? 600 : undefined}
            aria-pressed={isSelected}
            onClick={() => select(originalIndex)}
          >
            {formatDateTime(metric.timestamp)}
          </UnstyledButton>
        )
      }

      if (field === 'count') {
        return metric.count
      }

      return formatMetricValue(
        field === 'value' ? getSortValue(metric, 'value', metricType) : metric[field],
        metricType
      )
    },
  }))

  return (
    <Paper>
      <Text size="sm" fw={600} p="md" pb="xs">
        Data points ({data.metrics.length} periods)
      </Text>
      <VirtualTable
        records={sortedMetrics}
        columns={tableColumns}
        getKey={getKey}
        height={TABLE_HEIGHT}
        rowHeight={ROW_HEIGHT}
        label="Data points"
        sort={sort ? { columnId: sort.field, direction: sort.direction } : undefined}
        onSortChange={({ columnId }) => {
          if (isOneOf(SORT_FIELDS, columnId)) {
            handleSort(columnId)
          }
        }}
        rowAttributes={({ originalIndex }) => ({
          style: {
            height: ROW_HEIGHT,
            verticalAlign: 'middle',
            background: selectedIndex === originalIndex ? 'var(--mantine-color-blue-light)' : undefined,
          },
        })}
      />
    </Paper>
  )
}
