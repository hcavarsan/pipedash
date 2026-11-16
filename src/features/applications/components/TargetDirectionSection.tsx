import { Button, Group, Select, Stack, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import type { ChipArrow } from '../../../types/applications'

import type { GroupOption } from '../lib/targetGroups'
import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

interface TargetDirectionSectionProps extends Pick<ApplicationModalProps, 'detail' | 'copy'> {
  groups: GroupOption[]
}

export function TargetDirectionSection({ detail, copy, groups }: TargetDirectionSectionProps): ReactNode {
  const [from, setFrom] = useState<string | null>(null)
  const [to, setTo] = useState<string | null>(null)
  const edit = useEditApplication()
  const id = detail.summary.id
  const labelOf = (key: string): string => groups.find((group) => group.value === key)?.label ?? key

  const add = (): void => {
    if (from !== null && to !== null) {
      edit.mutate(
        { id, edit: { type: 'direction', from_group: from, to_group: to, present: true } },
        {
          onSuccess: () => {
            setFrom(null)
            setTo(null)
          },
        }
      )
    }
  }
  const removeArrow = (arrow: ChipArrow): void => {
    edit.mutate({ id, edit: { type: 'direction', from_group: arrow.from, to_group: arrow.to, present: false } })
  }

  return (
    <Stack gap="xs">
      {detail.summary.arrows.map((arrow) => (
        <Group key={`${arrow.from}>${arrow.to}`} gap="xs" justify="space-between" wrap="nowrap">
          <Text size="sm" truncate>
            {labelOf(arrow.from)} → {labelOf(arrow.to)}
          </Text>
          <Button size="xs" variant="default" disabled={edit.isPending} onClick={() => removeArrow(arrow)}>
            {copy.remove}
          </Button>
        </Group>
      ))}
      <Group gap="xs" align="flex-end" wrap="nowrap">
        <Select label={copy.direction_from} data={groups} value={from} onChange={setFrom} flex={1} miw={0} />
        <Select label={copy.direction_to} data={groups} value={to} onChange={setTo} flex={1} miw={0} />
        <Button loading={edit.isPending} disabled={from === null || to === null || from === to} onClick={add}>
          {copy.add_direction}
        </Button>
      </Group>
    </Stack>
  )
}
