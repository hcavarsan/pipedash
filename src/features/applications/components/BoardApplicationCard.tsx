import { Anchor, Badge, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import type { BoardApplication, BoardCopy, BoardLane } from '../../../types/applicationBoard'
import { DropArea, LaneArea, TargetCard } from './BoardParts'

interface BoardApplicationCardProps {
  application: BoardApplication
  copy: BoardCopy
  busy: boolean
}

function ColumnTitle({ children }: { children: string }): ReactNode {
  return (
    <Text size="xs" fw={600} c="dimmed" tt="uppercase">
      {children}
    </Text>
  )
}

function Lane({ lane, busy }: { lane: BoardLane; busy: boolean }): ReactNode {
  return (
    <Stack gap="2xs">
      <ColumnTitle>{lane.title}</ColumnTitle>
      <LaneArea lane={lane} busy={busy} />
    </Stack>
  )
}

function Environments({ application, copy, busy }: BoardApplicationCardProps): ReactNode {
  return (
    <Stack gap="2xs">
      <ColumnTitle>{copy.environments}</ColumnTitle>
      {application.environments.length === 0 && (
        <Text size="xs" c="dimmed">
          {copy.environments_empty}
        </Text>
      )}
      {application.environments.map((environment) => (
        <DropArea key={environment.zone.id} zone={environment.zone} busy={busy}>
          <Text size="sm" fw={500}>
            {environment.label}
          </Text>
          {environment.targets.map((target) => (
            <TargetCard key={target.card.item.key} target={target} busy={busy} />
          ))}
        </DropArea>
      ))}
      {application.new_environment && <DropArea zone={application.new_environment} busy={busy} />}
    </Stack>
  )
}

export function BoardApplicationCard({ application, copy, busy }: BoardApplicationCardProps): ReactNode {
  return (
    <Paper withBorder p="sm" component="section" aria-label={application.name}>
      <Stack gap="sm">
        <Group gap="xs" wrap="nowrap">
          <Anchor
            component={Link}
            to={`/applications/${application.id}`}
            fw={600}
            size="sm"
            truncate
            title={copy.open_application}
          >
            {application.name}
          </Anchor>
          {application.notice && (
            <Badge variant="default" size="sm">
              {application.notice}
            </Badge>
          )}
        </Group>
        <SimpleGrid cols={3} spacing="sm">
          <Lane lane={application.build} busy={busy} />
          <Lane lane={application.deploy} busy={busy} />
          <Environments application={application} copy={copy} busy={busy} />
        </SimpleGrid>
      </Stack>
    </Paper>
  )
}
