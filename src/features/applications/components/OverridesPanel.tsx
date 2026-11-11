import { Badge, Button, Group, Stack, Text, Title } from '@mantine/core'
import type { ReactNode } from 'react'

import type { ApplicationCopy, OverrideEntry } from '../../../types/applications'
import { formatDateTime } from '../../../utils/formatDateTime'
import { useRevertOverride } from '../queries/useApplicationQueries'

interface OverridesPanelProps {
  applicationId: number
  overrides: OverrideEntry[]
  copy: ApplicationCopy
}

interface OverrideRowProps {
  entry: OverrideEntry
  copy: ApplicationCopy
  reverting: boolean
  onRevert: () => void
}

function OverrideRow({ entry, copy, reverting, onRevert }: OverrideRowProps): ReactNode {
  return (
    <Group gap="sm" wrap="nowrap" justify="space-between" data-override-id={entry.id}>
      <Stack gap={0} miw={0}>
        <Group gap="xs">
          <Text size="sm">{entry.description}</Text>
          {entry.shadowed && (
            <Badge size="xs" variant="light" color="gray">
              {copy.shadowed}
            </Badge>
          )}
        </Group>
        <Text size="xs" c="dimmed">
          {[entry.author, formatDateTime(entry.created_at)].filter(Boolean).join(' · ')}
        </Text>
      </Stack>
      <Button size="xs" variant="default" loading={reverting} onClick={onRevert}>
        {copy.revert}
      </Button>
    </Group>
  )
}

export function OverridesPanel({ applicationId, overrides, copy }: OverridesPanelProps): ReactNode {
  const revert = useRevertOverride()

  return (
    <Stack gap="xs">
      <Title order={3} size="h5">
        {copy.overrides}
      </Title>
      {overrides.length === 0 && (
        <Text size="sm" c="dimmed">
          {copy.overrides_empty}
        </Text>
      )}
      {overrides.map((entry) => (
        <OverrideRow
          key={entry.id}
          entry={entry}
          copy={copy}
          reverting={revert.isPending && revert.variables?.overrideId === entry.id}
          onRevert={() => revert.mutate({ id: applicationId, overrideId: entry.id })}
        />
      ))}
    </Stack>
  )
}
