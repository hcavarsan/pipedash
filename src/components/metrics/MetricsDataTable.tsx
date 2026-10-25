import { Group, Menu, Paper, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconCheck, IconChevronDown, IconChevronUp, IconSelector } from '@tabler/icons-react'
import { useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { AggregatedMetric, AggregatedMetrics } from '../../types'
import { formatDateTime } from '../../utils/formatDateTime'
import { StandardTable } from '../common/StandardTable'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../ios/IosList'

import classes from './metrics.module.css'
import { formatMetricValue, formatNumber, hasObservation, METRIC_CONFIG } from './metricsUtils'

interface MetricsDataTableProps {
  data: AggregatedMetrics
  selectedIndex: number | null
  onRowClick: (index: number | null) => void
}

type SortField = 'timestamp' | 'value' | 'count' | 'min' | 'max'
type SortDirection = 'asc' | 'desc'

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
                subtitle={<span className={classes.num}>{`Count ${formatNumber(metric.count)} · ${range}`}</span>}
                value={
                  <span className={classes.num}>
                    {formatMetricValue(hasObservation(metric, metricType) ? metric.value : null, metricType)}
                  </span>
                }
                accessory={isSelected ? <IconCheck size={18} color="var(--ios-tint)" aria-hidden /> : undefined}
                selected={isSelected}
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
      <StandardTable
        records={sortedMetrics}
        idAccessor="timestamp"
        minHeight={120}
        height="auto"
        styles={{ table: { minWidth: 520 } }}
        striped
        rowStyle={(metric) => ({
          background: selectedIndex === data.metrics.indexOf(metric) ? 'var(--mantine-color-blue-light)' : undefined,
        })}
        columns={columns.map(({ field, label, numeric }) => ({
          accessor: field,
          title: (
            <UnstyledButton
              className="interactive-surface"
              onClick={() => handleSort(field)}
              aria-label={`Sort by ${label}${sort?.field === field ? `, ${sort.direction === 'asc' ? 'ascending' : 'descending'}` : ''}`}
              fz="sm"
              fw={600}
            >
              <Group gap="3xs" wrap="nowrap" justify={numeric ? 'flex-end' : 'flex-start'}>
                {label}
                {sort?.field === field ? (
                  sort.direction === 'asc' ? (
                    <IconChevronUp size={14} aria-hidden />
                  ) : (
                    <IconChevronDown size={14} aria-hidden />
                  )
                ) : (
                  <IconSelector size={14} aria-hidden />
                )}
              </Group>
            </UnstyledButton>
          ),
          textAlign: numeric ? 'right' : 'left',
          render: (metric: AggregatedMetric) => {
            if (field === 'timestamp') {
              const originalIndex = data.metrics.indexOf(metric)
              const isSelected = selectedIndex === originalIndex

              return (
                <UnstyledButton
                  className="interactive-surface"
                  fz="sm"
                  fw={isSelected ? 600 : undefined}
                  aria-pressed={isSelected}
                  onClick={() => onRowClick(isSelected ? null : originalIndex)}
                >
                  {formatDateTime(metric.timestamp)}
                </UnstyledButton>
              )
            }

            if (field === 'count') {
              return metric.count
            }

            return formatMetricValue(
              field === 'value' ? (hasObservation(metric, metricType) ? metric.value : null) : metric[field],
              metricType
            )
          },
        }))}
      />
    </Paper>
  )
}
