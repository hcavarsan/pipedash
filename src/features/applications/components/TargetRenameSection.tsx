import { Button, Group, Select, TextInput } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import type { GroupOption } from '../lib/targetGroups'
import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

interface TargetRenameSectionProps extends Pick<ApplicationModalProps, 'detail' | 'copy'> {
  groups: GroupOption[]
}

export function TargetRenameSection({ detail, copy, groups }: TargetRenameSectionProps): ReactNode {
  const [groupKey, setGroupKey] = useState<string | null>(null)
  const [name, setName] = useState('')
  const edit = useEditApplication()
  const trimmed = name.trim()
  const save = (): void => {
    if (groupKey !== null) {
      edit.mutate(
        { id: detail.summary.id, edit: { type: 'target_name', group_key: groupKey, name: trimmed } },
        { onSuccess: () => setName('') }
      )
    }
  }

  return (
    <Group gap="xs" align="flex-end" wrap="nowrap">
      <Select label={copy.targets} data={groups} value={groupKey} onChange={setGroupKey} flex={1} miw={0} />
      <TextInput
        label={copy.name}
        value={name}
        onChange={(event) => setName(event.currentTarget.value)}
        flex={1}
        miw={0}
      />
      <Button loading={edit.isPending} disabled={groupKey === null || trimmed.length === 0} onClick={save}>
        {copy.rename}
      </Button>
    </Group>
  )
}
