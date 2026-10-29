import { ActionIcon, Group, Stack, Text, Title, Tooltip } from '@mantine/core'
import { IconGripVertical, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { ViewMember } from '../../../types/domain'
import { SortableList } from './SortableList'

interface ViewMembersFieldsProps {
  members: ViewMember[]
  onChange: (ids: string[]) => void
}

export function ViewMembersFields({ members, onChange }: ViewMembersFieldsProps): ReactNode {
  return (
    <Stack gap="sm">
      <Title order={3} size="sm">
        Members
      </Title>
      {members.length === 0 ? (
        <Text size="sm" c="dimmed">
          Use Add to view on any pipeline to add it here.
        </Text>
      ) : (
        <Stack gap="4xs">
          <SortableList items={members} getId={(m) => m.pipeline_id} orientation="vertical" handle onReorder={onChange}>
            {(member, drag) => {
              const label = member.name ?? member.pipeline_id

              return (
                <Group
                  ref={drag.setNodeRef}
                  style={drag.style}
                  gap="xs"
                  wrap="nowrap"
                  opacity={drag.isDragging ? 0.6 : undefined}
                >
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
                    <ActionIcon
                      aria-label={`Remove ${label}`}
                      onClick={() =>
                        onChange(members.map((m) => m.pipeline_id).filter((id) => id !== member.pipeline_id))
                      }
                    >
                      <IconX size={16} aria-hidden />
                    </ActionIcon>
                  </Tooltip>
                </Group>
              )
            }}
          </SortableList>
        </Stack>
      )}
    </Stack>
  )
}
