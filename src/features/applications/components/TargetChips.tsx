import { Badge, Box, Group, Paper, Text } from '@mantine/core'
import { IconArrowRight } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import type { ChipArrow, TargetChip } from '../../../types/applications'

interface TargetChipsProps {
  chips: TargetChip[]
  arrows: ChipArrow[]
}

function ChipView({ chip }: { chip: TargetChip }): ReactNode {
  return (
    <Paper withBorder radius="sm" px="xs" py="3xs">
      <Group gap="xs" wrap="nowrap">
        <StatusIcon status={chip.status} />
        <Text size="xs" fw={500}>
          {chip.label}
        </Text>
        {chip.commit_short && (
          <Text size="xs" c="dimmed" ff="monospace">
            {chip.commit_short}
          </Text>
        )}
        {chip.health_label && (
          <Text size="xs" c="dimmed">
            {chip.health_label}
          </Text>
        )}
        {chip.behind_label && (
          <Badge variant="default" size="sm">
            {chip.behind_label}
          </Badge>
        )}
        {chip.kind !== 'target' && (
          <Badge variant="light" color="gray" size="sm">
            {chip.count}
          </Badge>
        )}
      </Group>
    </Paper>
  )
}

function ArrowView({ arrow, chips }: { arrow: ChipArrow; chips: TargetChip[] }): ReactNode {
  const from = chips.find((chip) => chip.group_key === arrow.from)
  const to = chips.find((chip) => chip.group_key === arrow.to)

  if (!from || !to) {
    return null
  }

  const label = `${from.label} → ${to.label}`

  return (
    <Box component="span" role="img" aria-label={label} title={label} display="inline-flex" c="dimmed">
      <IconArrowRight size={14} aria-hidden />
    </Box>
  )
}

export function TargetChips({ chips, arrows }: TargetChipsProps): ReactNode {
  return (
    <Group gap="xs" wrap="wrap">
      {chips.map((chip) => (
        <Group key={chip.group_key} gap="xs" wrap="nowrap">
          <ChipView chip={chip} />
          {arrows
            .filter((arrow) => arrow.from === chip.group_key)
            .map((arrow) => (
              <ArrowView key={`${arrow.from}>${arrow.to}`} arrow={arrow} chips={chips} />
            ))}
        </Group>
      ))}
    </Group>
  )
}
