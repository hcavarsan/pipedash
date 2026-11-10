import { ActionIcon, Group, Stack, Text, Title, Tooltip } from '@mantine/core'
import { IconGripVertical, IconX } from '@tabler/icons-react'
import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'

import type { ViewMember } from '../../../types/views'
import { QueryBoundary } from './QueryBoundary'
import { type SortableDragProps, SortableList } from './SortableList'

interface ViewMembersFieldsProps {
  query: UseQueryResult<ViewMember[], Error>
  members: ViewMember[]
  onChange: (ids: string[]) => void
}

interface ViewMemberRowProps {
  member: ViewMember
  drag: SortableDragProps
  onRemove: (pipelineId: string) => void
}

function ViewMemberRow({ member, drag, onRemove }: ViewMemberRowProps): ReactNode {
  const label = member.name ?? member.pipeline_id

  return (
    <Group ref={drag.setNodeRef} style={drag.style} gap="xs" wrap="nowrap" opacity={drag.isDragging ? 0.6 : undefined}>
      <ActionIcon
        {...drag.handleProps}
        {...drag.listeners}
        aria-label={`Reorder ${label}`}
        style={{ touchAction: 'none', cursor: 'grab' }}
      >
        <IconGripVertical size={16} aria-hidden />
      </ActionIcon>
      <Stack gap={0} flex={1} miw={0}>
        <Text size="sm" truncate>
          {label}
        </Text>
        {!member.available && (
          <Text size="xs" c="dimmed">
            Unavailable
          </Text>
        )}
      </Stack>
      <Tooltip label={`Remove ${label}`}>
        <ActionIcon aria-label={`Remove ${label}`} onClick={() => onRemove(member.pipeline_id)}>
          <IconX size={16} aria-hidden />
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}

function MemberList({ members, onChange }: Pick<ViewMembersFieldsProps, 'members' | 'onChange'>): ReactNode {
  const ids = members.map((m) => m.pipeline_id)

  if (members.length === 0) {
    return (
      <Text size="sm" c="dimmed">
        Use Add to view on any pipeline to add it here.
      </Text>
    )
  }

  return (
    <Stack gap="4xs">
      <SortableList items={members} getId={(m) => m.pipeline_id} orientation="vertical" handle onReorder={onChange}>
        {(member, drag) => (
          <ViewMemberRow member={member} drag={drag} onRemove={(pid) => onChange(ids.filter((id) => id !== pid))} />
        )}
      </SortableList>
    </Stack>
  )
}

export function ViewMembersFields({ query, members, onChange }: ViewMembersFieldsProps): ReactNode {
  return (
    <Stack gap="sm">
      <Title order={3} size="sm">
        Members
      </Title>
      <QueryBoundary query={query} loading="Loading members…" failure="Couldn't load view members" variant="compact">
        {() => <MemberList members={members} onChange={onChange} />}
      </QueryBoundary>
    </Stack>
  )
}
