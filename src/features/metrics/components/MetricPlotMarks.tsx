import { Paper, Stack, Text } from '@mantine/core'
import type { KeyboardEvent, ReactNode, SyntheticEvent } from 'react'
import { formatMetricValue } from '../lib/metricsUtils'
import type { BarShapeProps, CustomDotProps, CustomTooltipProps } from './metricPlotTypes'
import classes from './metrics.module.css'

const getSelectProps = (onSelect: (() => void) | undefined, label: string, isSelected: boolean, shell: boolean) => {
  if (!onSelect) {
    return {}
  }

  const handleSelect = (event: SyntheticEvent) => {
    event.stopPropagation()
    onSelect()
  }

  return {
    role: 'button',
    tabIndex: 0,
    'aria-label': label,
    'aria-pressed': isSelected,
    className: shell ? classes.point : classes.desktopPoint,
    onClick: handleSelect,
    onKeyDown: (event: KeyboardEvent) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault()
        handleSelect(event)
      }
    },
    style: { cursor: 'pointer' },
  }
}

function getDotRadius(isSelected: boolean, small: boolean): number {
  if (isSelected) {
    return 6
  }
  return small ? 2 : 4
}

export const CustomDot = ({
  cx,
  cy,
  payload,
  color,
  onSelect,
  small = false,
  shell = false,
}: CustomDotProps): ReactNode => {
  if (typeof cx !== 'number' || typeof cy !== 'number' || !payload || payload.value === null) {
    return null
  }

  const { isSelected, index, label } = payload

  return (
    <g {...getSelectProps(onSelect && (() => onSelect(index)), label, isSelected, shell)}>
      {onSelect && <circle className={shell ? classes.ring : classes.desktopRing} cx={cx} cy={cy} r={15} fill="none" />}
      {isSelected && <circle cx={cx} cy={cy} r={10} fill={color} fillOpacity={0.2} stroke="none" />}
      <circle
        cx={cx}
        cy={cy}
        r={getDotRadius(isSelected, small)}
        fill={isSelected ? color : 'var(--mantine-color-body)'}
        stroke={color}
        strokeWidth={isSelected ? 3 : 2}
      />
    </g>
  )
}

export const CustomTooltip = ({ label, payload, metricType }: CustomTooltipProps): ReactNode => {
  const point = payload?.[0]?.payload

  if (!point) {
    return null
  }

  return (
    <Paper p="xs" withBorder shadow="md" miw={150}>
      <Stack gap="3xs">
        <Text size="xs" fw={600}>
          {String(label)}
        </Text>
        <Text size="sm" fw={500}>
          {point.value === null ? 'No runs in this period' : formatMetricValue(point.value, metricType)}
        </Text>
      </Stack>
    </Paper>
  )
}

export function MetricBarShape({
  fill,
  x,
  y,
  width,
  height,
  payload,
  mobileShell,
  onSelect,
}: BarShapeProps & { mobileShell: boolean; onSelect: ((index: number) => void) | undefined }): ReactNode {
  const selectProps = getSelectProps(
    onSelect && payload && (() => onSelect(payload.index)),
    payload?.label ?? '',
    !!payload?.isSelected,
    mobileShell
  )

  return (
    <g {...selectProps}>
      {onSelect && (
        <rect
          className={mobileShell ? classes.ring : classes.desktopRing}
          x={(x ?? 0) - 3}
          y={(y ?? 0) - 3}
          width={(width ?? 0) + 6}
          height={(height ?? 0) + 6}
          rx={7}
          ry={7}
          fill="none"
        />
      )}
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        fill={fill}
        fillOpacity={payload?.isSelected ? 1 : 0.8}
        stroke={payload?.isSelected ? 'var(--mantine-color-text)' : 'none'}
        strokeWidth={payload?.isSelected ? 2 : 0}
        rx={4}
        ry={4}
      />
    </g>
  )
}
