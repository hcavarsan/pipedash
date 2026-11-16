import { Button, Group, MultiSelect, Select, Stack, TextInput } from '@mantine/core'
import { type ReactNode, useId, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { StandardModal } from '../../../components/common/StandardModal'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import type { ApplicationCopy, Stage } from '../../../types/applications'
import { useCreateApplication } from '../queries/useApplicationQueries'

interface CreateApplicationModalProps {
  opened: boolean
  onClose: () => void
  copy: ApplicationCopy
}

const stageOf = (value: string | null): Stage | null => (value === 'build' || value === 'deploy' ? value : null)

export function CreateApplicationModal({ opened, onClose, copy }: CreateApplicationModalProps): ReactNode {
  const formId = useId()
  const navigate = useNavigate()
  const create = useCreateApplication()
  const { data: pipelines = [] } = usePipelines()
  const [name, setName] = useState('')
  const [selected, setSelected] = useState<string[]>([])
  const [stages, setStages] = useState<Record<string, Stage | null>>({})
  const stageOptions = [
    { value: 'build', label: copy.stage_build },
    { value: 'deploy', label: copy.stage_deploy },
  ]
  const canSave = name.trim() !== ''

  const close = (): void => {
    setName('')
    setSelected([])
    setStages({})
    onClose()
  }

  const save = (): void => {
    if (!canSave) {
      return
    }

    create.mutate(
      { name: name.trim(), members: selected.map((id) => ({ pipeline_id: id, stage: stages[id] ?? null })) },
      {
        onSuccess: (detail) => {
          close()
          void navigate(`/applications/${detail.summary.id}`)
        },
      }
    )
  }

  const footer = (requestClose: () => void): ReactNode => (
    <Group justify="flex-end" gap="sm">
      <Button variant="default" size="sm" onClick={requestClose} disabled={create.isPending}>
        {copy.cancel}
      </Button>
      <Button type="submit" form={formId} size="sm" loading={create.isPending} disabled={!canSave}>
        {copy.save}
      </Button>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={close}
      size="md"
      title={copy.new_application}
      loading={create.isPending}
      footer={footer}
      primaryAction={{ label: copy.save, onClick: save, loading: create.isPending, disabled: !canSave }}
    >
      <form
        id={formId}
        onSubmit={(event) => {
          event.preventDefault()
          save()
        }}
      >
        <Stack gap="md">
          <TextInput
            label={copy.name}
            required
            maxLength={60}
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
            data-autofocus
          />
          <MultiSelect
            label={copy.members}
            placeholder={selected.length > 0 ? undefined : copy.add_member}
            searchable
            data={pipelines.map((pipeline) => ({ value: pipeline.id, label: pipeline.name }))}
            value={selected}
            onChange={setSelected}
          />
          {selected.map((id) => (
            <Select
              key={id}
              label={pipelines.find((pipeline) => pipeline.id === id)?.name ?? id}
              placeholder={copy.stage}
              data={stageOptions}
              value={stages[id] ?? null}
              clearable
              onChange={(value) => setStages((previous) => ({ ...previous, [id]: stageOf(value) }))}
            />
          ))}
        </Stack>
      </form>
    </StandardModal>
  )
}
