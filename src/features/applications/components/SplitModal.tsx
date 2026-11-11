import { MultiSelect, Stack, TextInput } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

export function SplitModal({ detail, copy, onClose }: ApplicationModalProps): ReactNode {
  const [name, setName] = useState('')
  const [pipelineIds, setPipelineIds] = useState<string[]>([])
  const edit = useEditApplication()
  const options = detail.members.map((member) => ({ value: member.pipeline.id, label: member.pipeline.name }))
  const trimmed = name.trim()
  const save = (): void => {
    edit.mutate(
      { id: detail.summary.id, edit: { type: 'split', pipeline_ids: pipelineIds, name: trimmed } },
      { onSuccess: onClose }
    )
  }

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={copy.split}
      primaryAction={{
        label: copy.split,
        loading: edit.isPending,
        disabled: trimmed.length === 0 || pipelineIds.length === 0,
        onClick: save,
      }}
    >
      <Stack gap="sm">
        <TextInput
          label={copy.name}
          value={name}
          onChange={(event) => setName(event.currentTarget.value)}
          data-autofocus
        />
        <MultiSelect label={copy.members} data={options} value={pipelineIds} onChange={setPipelineIds} searchable />
      </Stack>
    </StandardModal>
  )
}
