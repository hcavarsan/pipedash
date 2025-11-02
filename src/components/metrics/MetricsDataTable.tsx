import { useState } from 'react'

import { Group, Paper, ScrollArea, Stack, Table, Text } from '@mantine/core'
import { IconChevronDown, IconChevronUp, IconSelector } from '@tabler/icons-react'

import type { AggregatedMetrics } from '../../types'
import { formatDuration } from '../../utils/formatDuration'

interface MetricsDataTableProps {
  data: AggregatedMetrics | null
  selectedIndex: number | null
  onRowClick: (index: number | null) => void
}

type SortField = 'timestamp' | 'value' | 'count' | 'min' | 'max'
type SortDirection = 'asc' | 'desc' | null

export const MetricsDataTable = ({ data, selectedIndex, onRowClick }: MetricsDataTableProps) => {
  const [sortField, setSortField] = useState<SortField | null>(null)
  const [sortDirection, setSortDirection] = useState<SortDirection>(null)

  if (!data || data.metrics.length === 0) {
    return (
      <Paper p="md" withBorder>
        <Text size="sm" c="dimmed" ta="center">
          No data available
        </Text>
      </Paper>
    )
  }

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      if (sortDirection === 'asc') {
        setSortDirection('desc')
      } else if (sortDirection === 'desc') {
        setSortField(null)
        setSortDirection(null)
      } else {
        setSortDirection('asc')
      }
    } else {
      setSortField(field)
      setSortDirection('asc')
    }
  }

  const getSortIcon = (field: SortField) => {
    if (sortField !== field) {
      return <IconSelector size={14} />
    }

return sortDirection === 'asc' ? <IconChevronUp size={14} /> : <IconChevronDown size={14} />
  }

  const sortedMetrics = [...data.metrics]


  if (sortField && sortDirection) {
    sortedMetrics.sort((a, b) => {
      let aVal: any
      let bVal: any

      switch (sortField) {
        case 'timestamp':
          aVal = new Date(a.timestamp).getTime()
          bVal = new Date(b.timestamp).getTime()
          break
        case 'value':
          aVal = a.value
          bVal = b.value
          break
        case 'count':
          aVal = a.count
          bVal = b.count
          break
        case 'min':
          aVal = a.min ?? -Infinity
          bVal = b.min ?? -Infinity
          break
        case 'max':
          aVal = a.max ?? -Infinity
          bVal = b.max ?? -Infinity
          break
      }

      if (sortDirection === 'asc') {
        return aVal < bVal ? -1 : aVal > bVal ? 1 : 0
      } else {
        return aVal > bVal ? -1 : aVal < bVal ? 1 : 0
      }
    })
  }

  const formatValue = (value: number): string => {
    if (data.metric_type === 'run_duration') {
      return formatDuration(value)
    }
    if (data.metric_type === 'success_rate') {
      return `${value.toFixed(1)}%`
    }

return value.toFixed(0)
  }

  const getValueLabel = (): string => {
    switch (data.metric_type) {
      case 'run_duration':
        return 'Avg Duration'
      case 'success_rate':
        return 'Success Rate'
      case 'run_frequency':
        return 'Run Count'
    }
  }

  return (
    <Paper withBorder>
      <Stack gap={0}>
        <Text size="sm" fw={600} p="md" pb="xs">
          Data Points ({data.metrics.length} total)
        </Text>
        <ScrollArea h={400}>
          <Table striped highlightOnHover>
            <Table.Thead>
            <Table.Tr>
              <Table.Th
                onClick={() => handleSort('timestamp')}
                style={{ cursor: 'pointer', userSelect: 'none' }}
              >
                <Group gap={4}>
                  <Text size="sm">Date</Text>
                  {getSortIcon('timestamp')}
                </Group>
              </Table.Th>
              <Table.Th
                onClick={() => handleSort('value')}
                style={{ cursor: 'pointer', userSelect: 'none' }}
              >
                <Group gap={4}>
                  <Text size="sm">{getValueLabel()}</Text>
                  {getSortIcon('value')}
                </Group>
              </Table.Th>
              <Table.Th
                onClick={() => handleSort('count')}
                style={{ cursor: 'pointer', userSelect: 'none' }}
              >
                <Group gap={4}>
                  <Text size="sm">Count</Text>
                  {getSortIcon('count')}
                </Group>
              </Table.Th>
              <Table.Th
                onClick={() => handleSort('min')}
                style={{ cursor: 'pointer', userSelect: 'none' }}
              >
                <Group gap={4}>
                  <Text size="sm">Min</Text>
                  {getSortIcon('min')}
                </Group>
              </Table.Th>
              <Table.Th
                onClick={() => handleSort('max')}
                style={{ cursor: 'pointer', userSelect: 'none' }}
              >
                <Group gap={4}>
                  <Text size="sm">Max</Text>
                  {getSortIcon('max')}
                </Group>
              </Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {sortedMetrics.map((metric) => {
              const originalIndex = data.metrics.indexOf(metric)
              const isSelected = selectedIndex === originalIndex



return (
                <Table.Tr
                  key={originalIndex}
                  onClick={() => onRowClick(isSelected ? null : originalIndex)}
                  style={{
                    cursor: 'pointer',
                    backgroundColor: isSelected ? 'var(--mantine-color-blue-light)' : undefined,
                    borderLeft: isSelected ? '3px solid var(--mantine-color-blue-6)' : '3px solid transparent',
                    fontWeight: isSelected ? 600 : undefined,
                    transition: 'all 0.15s ease',
                  }}
                >
                  <Table.Td>{new Date(metric.timestamp).toLocaleString()}</Table.Td>
                  <Table.Td>{formatValue(metric.value)}</Table.Td>
                  <Table.Td>{metric.count}</Table.Td>
                  <Table.Td>{metric.min !== null && metric.min !== undefined ? formatValue(metric.min) : '-'}</Table.Td>
                  <Table.Td>{metric.max !== null && metric.max !== undefined ? formatValue(metric.max) : '-'}</Table.Td>
                </Table.Tr>
              )
            })}
          </Table.Tbody>
        </Table>
        </ScrollArea>
      </Stack>
    </Paper>
  )
}
