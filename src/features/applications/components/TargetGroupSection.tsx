import { Button, Group, MultiSelect, Stack, TextInput } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

export function TargetGroupSection({ detail, copy }: Pick<ApplicationModalProps, 'detail' | 'copy'>): ReactNode {
  const [targetKeys, setTargetKeys] = useState<string[]>([])
  const [groupName, setGroupName] = useState('')
  const edit = useEditApplication()
  const trimmed = groupName.trim()
  const options = detail.targets.map((target) => ({ value: target.target_key, label: target.name }))
  const reset = (): void => {
    setTargetKeys([])
    setGroupName('')
  }
  const apply = (name: string | null): void => {
    edit.mutate(
      { id: detail.summary.id, edit: { type: 'target_group', target_keys: targetKeys, group_name: name } },
      { onSuccess: reset }
    )
  }

  return (
    <Stack gap="xs">
      <MultiSelect label={copy.targets} data={options} value={targetKeys} onChange={setTargetKeys} searchable />
      <Group gap="xs" align="flex-end" wrap="nowrap">
        <TextInput
          label={copy.group}
          value={groupName}
          onChange={(event) => setGroupName(event.currentTarget.value)}
          flex={1}
          miw={0}
        />
        <Button
          loading={edit.isPending}
          disabled={targetKeys.length === 0 || trimmed.length === 0}
          onClick={() => apply(trimmed)}
        >
          {copy.group}
        </Button>
        <Button
          variant="default"
          loading={edit.isPending}
          disabled={targetKeys.length === 0}
          onClick={() => apply(null)}
        >
          {copy.ungroup}
        </Button>
      </Group>
    </Stack>
  )
}
