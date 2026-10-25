import { Group, Menu, Paper, Stack, Table, Text, UnstyledButton } from '@mantine/core'
import { IconCheck, IconChevronDown, IconChevronUp, IconSelector } from '@tabler/icons-react'
import { useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { AggregatedMetric, AggregatedMetrics } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../ios/IosList'

import classes from './metrics.module.css'
import { formatMetricValue, hasObservation, METRIC_CONFIG } from './metricsUtils'

interface MetricsDataTableProps {
  data: AggregatedMetrics
  selectedIndex: number | null
  onRowClick: (index: number | null) => void
}

type SortField = 'timestamp' | 'value' | 'count' | 'min' | 'max'
type SortDirection = 'asc' | 'desc'

const SORT_ARIA: Record<SortDirection, 'ascending' | 'descending'> = { asc: 'ascending', desc: 'descending' }

export const MetricsDataTable = ({ data, selectedIndex, onRowClick }: MetricsDataTableProps) => {
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

  const getSortValue = (metric: AggregatedMetric, field: SortField): number | null => {
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

  const sortedMetrics = [...data.metrics]

  if (sort) {
    sortedMetrics.sort((a, b) => {
      const aValue = getSortValue(a, sort.field)
      const bValue = getSortValue(b, sort.field)

      if (aValue === null || bValue === null) {
        return Number(aValue === null) - Number(bValue === null)
      }

      return sort.direction === 'asc' ? aValue - bValue : bValue - aValue
    })
  }

  const getSortIcon = (field: SortField) => {
    if (sort?.field !== field) {
      return <IconSelector size={14} aria-hidden />
    }

    return sort.direction === 'asc' ? (
      <IconChevronUp size={14} aria-hidden />
    ) : (
      <IconChevronDown size={14} aria-hidden />
    )
  }

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
          {sortedMetrics.map((metric) => {
            const originalIndex = data.metrics.indexOf(metric)
            const isSelected = selectedIndex === originalIndex
            const range = `Min ${formatMetricValue(metric.min, metricType)} · Max ${formatMetricValue(metric.max, metricType)}`

            return (
              <IosRow
                key={metric.timestamp}
                title={formatDateTime(metric.timestamp)}
                subtitle={`Count ${metric.count} · ${range}`}
                value={formatMetricValue(hasObservation(metric, metricType) ? metric.value : null, metricType)}
                accessory={
                  isSelected ? <IconCheck size={18} color="var(--mantine-color-anchor)" aria-hidden /> : undefined
                }
                onClick={() => onRowClick(isSelected ? null : originalIndex)}
              />
            )
          })}
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Paper>
      <Text size="sm" fw={600} p="md" pb="xs">
        Data points ({data.metrics.length} periods)
      </Text>
      <Table.ScrollContainer minWidth={520}>
        <Table striped highlightOnHover style={{ fontVariantNumeric: 'tabular-nums' }}>
          <Table.Thead>
            <Table.Tr>
              {columns.map(({ field, label, numeric }) => (
                <Table.Th
                  key={field}
                  ta={numeric ? 'right' : 'left'}
                  aria-sort={sort?.field === field ? SORT_ARIA[sort.direction] : 'none'}
                >
                  <UnstyledButton onClick={() => handleSort(field)} fz="sm" fw={600}>
                    <Group gap={4} wrap="nowrap">
                      {numeric && getSortIcon(field)}
                      {label}
                      {!numeric && getSortIcon(field)}
                    </Group>
                  </UnstyledButton>
                </Table.Th>
              ))}
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {sortedMetrics.map((metric) => {
              const originalIndex = data.metrics.indexOf(metric)
              const isSelected = selectedIndex === originalIndex
              const date = formatDateTime(metric.timestamp)

              return (
                <Table.Tr key={metric.timestamp} bg={isSelected ? 'var(--mantine-color-blue-light)' : undefined}>
                  <Table.Td>
                    <UnstyledButton
                      fz="sm"
                      fw={isSelected ? 600 : undefined}
                      aria-pressed={isSelected}
                      onClick={() => onRowClick(isSelected ? null : originalIndex)}
                    >
                      {date}
                    </UnstyledButton>
                  </Table.Td>
                  <Table.Td ta="right">
                    {formatMetricValue(hasObservation(metric, metricType) ? metric.value : null, metricType)}
                  </Table.Td>
                  <Table.Td ta="right">{metric.count}</Table.Td>
                  <Table.Td ta="right">{formatMetricValue(metric.min, metricType)}</Table.Td>
                  <Table.Td ta="right">{formatMetricValue(metric.max, metricType)}</Table.Td>
                </Table.Tr>
              )
            })}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
    </Paper>
  )
}
