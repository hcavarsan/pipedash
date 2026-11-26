import { TextInput } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import type { BoardCopy } from '../../../types/applicationBoard'
import { useEditApplication } from '../queries/useApplicationQueries'
import { ActionModal } from './ActionModal'

interface NewEnvironmentModalProps {
  applicationId: number
  targetKey: string
  copy: BoardCopy
  onClose: () => void
}

export function NewEnvironmentModal({ applicationId, targetKey, copy, onClose }: NewEnvironmentModalProps): ReactNode {
  const [name, setName] = useState('')
  const edit = useEditApplication()
  const trimmed = name.trim()
  const save = (): void => {
    edit.mutate(
      { id: applicationId, edit: { type: 'target_group', target_keys: [targetKey], group_name: trimmed } },
      { onSuccess: onClose }
    )
  }

  return (
    <ActionModal
      onClose={onClose}
      title={copy.new_environment}
      cancel={copy.cancel}
      dirty={trimmed.length > 0}
      action={{
        label: copy.create_environment,
        loading: edit.isPending,
        disabled: trimmed.length === 0,
        onClick: save,
      }}
    >
      <TextInput
        label={copy.environment_name}
        maxLength={60}
        value={name}
        disabled={edit.isPending}
        onChange={(event) => setName(event.currentTarget.value)}
        data-autofocus
      />
    </ActionModal>
  )
}
