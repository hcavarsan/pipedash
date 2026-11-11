import { Paper, Stack, Text, UnstyledButton } from '@mantine/core'
import { type ReactNode, useMemo } from 'react'
import { VirtualList } from '../../../components/common/VirtualList'
import { VirtualTable, type VirtualTableColumn } from '../../../components/common/VirtualTable'
import { IosGroup } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { MetricSortField, MetricsDashboard, MetricTableSort } from '../../../types/metrics'
import { formatDateTime } from '../../../utils/formatDateTime'
import { isOneOf } from '../../../utils/isOneOf'
import { formatMetricValue, METRIC_CONFIG } from '../lib/metricsUtils'
import { type IndexedMetric, MetricShellRow } from './MetricShellRow'
import { MetricsTableSortMenu, type SortColumn } from './MetricsTableSortMenu'

interface MetricsDataTableProps {
  data: MetricsDashboard
  sort: MetricTableSort | undefined
  onSortChange: (sort: MetricTableSort | undefined) => void
  selectedIndex: number | null
  onRowClick: (index: number | null) => void
}

const SORT_FIELDS = ['timestamp', 'value', 'count', 'min', 'max'] as const

const ROW_HEIGHT = 44
const TABLE_HEIGHT = 480
const SHELL_ROW_HEIGHT = 64

const getKey = ({ originalIndex }: IndexedMetric): string => String(originalIndex)

export const MetricsDataTable = ({
  data,
  sort,
  onSortChange,
  selectedIndex,
  onRowClick,
}: MetricsDataTableProps): ReactNode => {
  const mobileShell = useMobileShell()
  const metricType = data.data.metric_type

  const columns: SortColumn[] = [
    { field: 'timestamp', label: 'Date', numeric: false },
    { field: 'value', label: METRIC_CONFIG[metricType].valueLabel, numeric: true },
    { field: 'count', label: 'Count', numeric: true },
    { field: 'min', label: 'Min', numeric: true },
    { field: 'max', label: 'Max', numeric: true },
  ]

  const handleSort = (field: MetricSortField) => {
    if (sort?.field !== field) {
      onSortChange({ field, direction: 'asc' })
    } else if (sort.direction === 'asc') {
      onSortChange({ field, direction: 'desc' })
    } else {
      onSortChange(undefined)
    }
  }

  const sortedMetrics = useMemo<IndexedMetric[]>(
    () =>
      data.table_indices.flatMap((originalIndex) => {
        const metric = data.data.metrics[originalIndex]

        return metric ? [{ metric, originalIndex, observed: data.observed[originalIndex] === true }] : []
      }),
    [data]
  )

  const select = (originalIndex: number) => onRowClick(selectedIndex === originalIndex ? null : originalIndex)

  const renderShellRow = (row: IndexedMetric): ReactNode => (
    <MetricShellRow
      row={row}
      metricType={metricType}
      selected={selectedIndex === row.originalIndex}
      onSelect={() => select(row.originalIndex)}
    />
  )

  if (mobileShell) {
    return (
      <Stack gap="lg">
        <IosGroup footer="Choose the same column again to reverse or clear the order.">
          <MetricsTableSortMenu columns={columns} sort={sort} onSort={handleSort} />
        </IosGroup>

        <IosGroup header={`Data points (${data.data.metrics.length} periods)`}>
          <VirtualList
            items={sortedMetrics}
            getKey={getKey}
            estimateSize={SHELL_ROW_HEIGHT}
            renderItem={renderShellRow}
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
    render: ({ metric, originalIndex, observed }) => {
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

      const value = field === 'value' && !observed ? null : metric[field]

      return formatMetricValue(value, metricType)
    },
  }))

  return (
    <Paper>
      <Text size="sm" fw={600} p="md" pb="xs">
        Data points ({data.data.metrics.length} periods)
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
