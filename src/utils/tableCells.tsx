import { Code, Group, Text } from '@mantine/core'

import { CopyButton } from '../components/atoms/CopyButton'
import { StatusBadge } from '../components/atoms/StatusBadge'
import { TruncatedText } from '../components/atoms/TruncatedText'
import type { PipelineStatus } from '../types'

import { formatDateTime } from './formatDateTime'

export const TABLE_CELL_STYLES = {
  text: {
    size: 'sm' as const,
    weight: {
      normal: 400,
      medium: 500,
      semibold: 600,
    },
    color: {
      default: undefined,
      dimmed: 'dimmed' as const,
    },
  },
  spacing: {
    gap: 8,
  },
  code: {
    size: 'sm' as const,
    weight: 500,
  },
} as const

export const TableCells = {
  text: (value: string | number) => (
    <Text size={TABLE_CELL_STYLES.text.size} fw={TABLE_CELL_STYLES.text.weight.normal}>
      {value}
    </Text>
  ),

  textBold: (value: string | number) => (
    <Text size={TABLE_CELL_STYLES.text.size} fw={TABLE_CELL_STYLES.text.weight.semibold}>
      {value}
    </Text>
  ),

  textDimmed: (value: string | number) => (
    <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
      {value}
    </Text>
  ),

  textDimmedMedium: (value: string | number) => (
    <Text
      size={TABLE_CELL_STYLES.text.size}
      c={TABLE_CELL_STYLES.text.color.dimmed}
      fw={TABLE_CELL_STYLES.text.weight.medium}
    >
      {value}
    </Text>
  ),

  truncated: (value: string) => (
    <TruncatedText size={TABLE_CELL_STYLES.text.size} fw={TABLE_CELL_STYLES.text.weight.medium}>
      {value}
    </TruncatedText>
  ),

  truncatedDimmed: (value: string | null | undefined) => (
    <TruncatedText
      size={TABLE_CELL_STYLES.text.size}
      c={TABLE_CELL_STYLES.text.color.dimmed}
      fw={TABLE_CELL_STYLES.text.weight.medium}
    >
      {value || '—'}
    </TruncatedText>
  ),

  status: (status: PipelineStatus) => <StatusBadge status={status} />,

  commit: (sha: string | null | undefined) =>
    sha ? (
      <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
        <Code fz={TABLE_CELL_STYLES.code.size} fw={TABLE_CELL_STYLES.code.weight}>
          {sha.substring(0, 7)}
        </Code>
        <CopyButton value={sha} label="Copy commit SHA" />
      </Group>
    ) : (
      <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
        —
      </Text>
    ),

  timestamp: (date: string | null) =>
    date ? (
      <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
        {formatDateTime(date)}
      </Text>
    ) : (
      <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
        Never
      </Text>
    ),

  iconText: (icon: React.ReactNode, text: string) => (
    <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
      {icon}
      <TruncatedText size={TABLE_CELL_STYLES.text.size} fw={TABLE_CELL_STYLES.text.weight.medium}>
        {text}
      </TruncatedText>
    </Group>
  ),
}
