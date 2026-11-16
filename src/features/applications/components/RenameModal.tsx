import { TextInput } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

export function RenameModal({ detail, copy, onClose }: ApplicationModalProps): ReactNode {
  const [name, setName] = useState(detail.summary.name)
  const edit = useEditApplication()
  const trimmed = name.trim()
  const save = (): void => {
    edit.mutate({ id: detail.summary.id, edit: { type: 'rename', name: trimmed } }, { onSuccess: onClose })
  }

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={copy.rename}
      primaryAction={{ label: copy.save, loading: edit.isPending, disabled: trimmed.length === 0, onClick: save }}
    >
      <TextInput
        label={copy.name}
        value={name}
        onChange={(event) => setName(event.currentTarget.value)}
        data-autofocus
      />
    </StandardModal>
  )
}
