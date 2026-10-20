import { Group, Paper, Table, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronUp, IconSelector } from '@tabler/icons-react'
import { useState } from 'react'

import type { AggregatedMetric, AggregatedMetrics } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'

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
  const [sort, setSort] = useState<{ field: SortField; direction: SortDirection } | null>(null)
  const metricType = data.metric_type

  const columns: { field: SortField; label: string }[] = [
    { field: 'timestamp', label: 'Date' },
    { field: 'value', label: METRIC_CONFIG[metricType].valueLabel },
    { field: 'count', label: 'Count' },
    { field: 'min', label: 'Min' },
    { field: 'max', label: 'Max' },
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

  return (
    <Paper>
      <Text size="sm" fw={600} p="md" pb="xs">
        Data points ({data.metrics.length} periods)
      </Text>
      <Table.ScrollContainer minWidth={520}>
        <Table striped highlightOnHover>
          <Table.Thead>
            <Table.Tr>
              {columns.map(({ field, label }) => (
                <Table.Th key={field} aria-sort={sort?.field === field ? SORT_ARIA[sort.direction] : 'none'}>
                  <UnstyledButton onClick={() => handleSort(field)} fz="sm" fw={600}>
                    <Group gap={4} wrap="nowrap">
                      {label}
                      {getSortIcon(field)}
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
                  <Table.Td>
                    {formatMetricValue(hasObservation(metric, metricType) ? metric.value : null, metricType)}
                  </Table.Td>
                  <Table.Td>{metric.count}</Table.Td>
                  <Table.Td>{formatMetricValue(metric.min, metricType)}</Table.Td>
                  <Table.Td>{formatMetricValue(metric.max, metricType)}</Table.Td>
                </Table.Tr>
              )
            })}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
    </Paper>
  )
}
