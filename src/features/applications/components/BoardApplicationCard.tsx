import { Anchor, Badge, Group, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import type { BoardApplication, BoardCopy, BoardPipeline } from '../../../types/applicationBoard'
import type { Stage } from '../../../types/applications'
import { DropArea, PipelineCard, TargetCard } from './BoardParts'

interface BoardApplicationCardProps {
  application: BoardApplication
  copy: BoardCopy
  busy: boolean
}

interface LaneProps extends BoardApplicationCardProps {
  stage: Stage
  title: string
  pipelines: BoardPipeline[]
}

function ColumnTitle({ children }: { children: string }): ReactNode {
  return (
    <Text size="xs" fw={600} c="dimmed" tt="uppercase">
      {children}
    </Text>
  )
}

function Lane({ application, copy, busy, stage, title, pipelines }: LaneProps): ReactNode {
  const locked = busy || !application.editable

  return (
    <Stack gap="2xs">
      <ColumnTitle>{title}</ColumnTitle>
      <DropArea
        zone={{ kind: 'lane', applicationId: application.id, stage }}
        label={`${application.name} ${title}`}
        disabled={locked}
        hint={pipelines.length === 0 && application.editable ? copy.lane_empty : undefined}
      >
        {pipelines.map((pipeline) => (
          <PipelineCard
            key={pipeline.pipeline_id}
            pipeline={pipeline}
            applicationId={application.id}
            stage={stage}
            disabled={locked}
          />
        ))}
      </DropArea>
    </Stack>
  )
}

function Environments({ application, copy, busy }: BoardApplicationCardProps): ReactNode {
  const locked = busy || !application.editable

  return (
    <Stack gap="2xs">
      <ColumnTitle>{copy.environments}</ColumnTitle>
      {application.environments.length === 0 && (
        <Text size="xs" c="dimmed">
          {copy.environments_empty}
        </Text>
      )}
      {application.environments.map((environment) => (
        <DropArea
          key={environment.group_key}
          zone={{
            kind: 'environment',
            applicationId: application.id,
            groupKey: environment.group_key,
            label: environment.label,
            targetKeys: environment.targets.map((target) => target.target_key),
          }}
          label={`${application.name} ${environment.label}`}
          disabled={locked}
        >
          <Text size="sm" fw={500}>
            {environment.label}
          </Text>
          {environment.targets.map((target) => (
            <TargetCard
              key={target.target_key}
              target={target}
              applicationId={application.id}
              groupKey={environment.group_key}
              disabled={locked}
            />
          ))}
        </DropArea>
      ))}
      {application.editable && application.environments.length > 0 && (
        <DropArea
          zone={{ kind: 'new_environment', applicationId: application.id }}
          label={`${application.name} ${copy.new_environment}`}
          disabled={busy}
          hint={copy.new_environment_drop}
        />
      )}
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
          <Lane
            application={application}
            copy={copy}
            busy={busy}
            stage="build"
            title={copy.build}
            pipelines={application.build}
          />
          <Lane
            application={application}
            copy={copy}
            busy={busy}
            stage="deploy"
            title={copy.deploy}
            pipelines={application.deploy}
          />
          <Environments application={application} copy={copy} busy={busy} />
        </SimpleGrid>
      </Stack>
    </Paper>
  )
}
