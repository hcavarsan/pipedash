import { Code, Group, Text } from '@mantine/core'

import { CopyButton } from '../components/atoms/CopyButton'
import { TruncatedText } from '../components/atoms/TruncatedText'

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
    gap: 'xs',
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

  textDimmed: (value: string | number) => (
    <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
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
}
