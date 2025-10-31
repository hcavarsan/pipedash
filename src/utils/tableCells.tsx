import { Avatar, Badge, Code, Group, Text, ThemeIcon } from '@mantine/core'

import { CopyButton } from '../components/atoms/CopyButton'
import { StatusBadge } from '../components/atoms/StatusBadge'
import { TruncatedText } from '../components/atoms/TruncatedText'
import type { PipelineStatus } from '../types'


const TABLE_CELL_STYLES = {
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
  badge: {
    size: 'md' as const,
    variant: 'light' as const,
  },
  icon: {
    size: 18,
    actionSize: 16,
  },
  spacing: {
    gap: 8,
  },
  code: {
    size: 'sm' as const,
    weight: 500,
  },
  avatar: {
    size: 20,
    radius: 'xs' as const,
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
    <TruncatedText
      size={TABLE_CELL_STYLES.text.size}
      fw={TABLE_CELL_STYLES.text.weight.medium}
    >
      {value}
    </TruncatedText>
  ),

  // Truncated dimmed text
  truncatedDimmed: (value: string) => (
    <TruncatedText
      size={TABLE_CELL_STYLES.text.size}
      c={TABLE_CELL_STYLES.text.color.dimmed}
      fw={TABLE_CELL_STYLES.text.weight.medium}
    >
      {value}
    </TruncatedText>
  ),

  status: (status: PipelineStatus) => (
    <StatusBadge
      status={status}
      size={TABLE_CELL_STYLES.badge.size}
    />
  ),

  countBadge: (count: number) => (
    <Badge
      variant={TABLE_CELL_STYLES.badge.variant}
      color="blue"
      size={TABLE_CELL_STYLES.badge.size}
    >
      {count}
    </Badge>
  ),

  commit: (sha: string) => (
    <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
      <Code fz={TABLE_CELL_STYLES.code.size} fw={TABLE_CELL_STYLES.code.weight}>
        {sha.substring(0, 7)}
      </Code>
      <CopyButton value={sha} size="sm" />
    </Group>
  ),

  timestamp: (date: string | null) => (
    date ? (
      <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
        {new Date(date).toLocaleString()}
      </Text>
    ) : (
      <Text size={TABLE_CELL_STYLES.text.size} c={TABLE_CELL_STYLES.text.color.dimmed}>
        Never
      </Text>
    )
  ),

  iconText: (icon: React.ReactNode, text: string) => (
    <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
      {icon}
      <TruncatedText size={TABLE_CELL_STYLES.text.size} fw={TABLE_CELL_STYLES.text.weight.medium}>
        {text}
      </TruncatedText>
    </Group>
  ),

  avatarName: (src: string | null, name: string, fallbackIcon?: React.ReactNode) => (
    <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
      {src ? (
        <Avatar
          src={src}
          size={TABLE_CELL_STYLES.avatar.size}
          radius={TABLE_CELL_STYLES.avatar.radius}
        >
          {fallbackIcon}
        </Avatar>
      ) : fallbackIcon ? (
        <ThemeIcon
          size={TABLE_CELL_STYLES.avatar.size}
          radius={TABLE_CELL_STYLES.avatar.radius}
          variant="light"
          color="gray"
        >
          {fallbackIcon}
        </ThemeIcon>
      ) : (
        <Avatar size={TABLE_CELL_STYLES.avatar.size} radius={TABLE_CELL_STYLES.avatar.radius} color="gray" />
      )}
      <Text size={TABLE_CELL_STYLES.text.size}>
        {name}
      </Text>
    </Group>
  ),
}
