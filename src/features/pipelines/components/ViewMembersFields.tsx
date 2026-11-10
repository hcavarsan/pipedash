import { ActionIcon, Group, Stack, Text, Title, Tooltip } from '@mantine/core'
import { IconGripVertical, IconX } from '@tabler/icons-react'
import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'

import { EmptyState } from '../../../components/common/EmptyState'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import type { ViewMember } from '../../../types/views'
import { type SortableDragProps, SortableList } from './SortableList'

interface ViewMembersFieldsProps {
  query: UseQueryResult<ViewMember[], Error>
  members: ViewMember[]
  disabled: boolean
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

type MemberListProps = Pick<ViewMembersFieldsProps, 'members' | 'disabled' | 'onChange'>

function MemberList({ members, disabled, onChange }: MemberListProps): ReactNode {
  const ids = members.map((m) => m.pipeline_id)

  if (members.length === 0) {
    return (
      <EmptyState title="No pipelines in this view" description="Use Add to view on any pipeline to add it here." />
    )
  }

  return (
    <Stack gap="4xs">
      <SortableList
        items={members}
        getId={(m) => m.pipeline_id}
        orientation="vertical"
        handle
        disabled={disabled}
        onReorder={onChange}
      >
        {(member, drag) => (
          <ViewMemberRow member={member} drag={drag} onRemove={(pid) => onChange(ids.filter((id) => id !== pid))} />
        )}
      </SortableList>
    </Stack>
  )
}

export function ViewMembersFields({ query, members, disabled, onChange }: ViewMembersFieldsProps): ReactNode {
  return (
    <Stack gap="sm">
      <Title order={3} size="sm">
        Members
      </Title>
      <QueryBoundary query={query} loading="Loading members…" failure="Couldn't load view members" variant="compact">
        {() => <MemberList members={members} disabled={disabled} onChange={onChange} />}
      </QueryBoundary>
    </Stack>
  )
}
