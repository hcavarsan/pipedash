import { Button, Group, Select, Stack, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import type { ApplicationCopy, ApplicationMember, Stage } from '../../../types/applications'
import { useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

const isStage = (value: string | null): value is Stage => value === 'build' || value === 'deploy'

function stageOptions(copy: ApplicationCopy): { value: Stage; label: string }[] {
  return [
    { value: 'build', label: copy.stage_build },
    { value: 'deploy', label: copy.stage_deploy },
  ]
}

interface MemberRowProps {
  member: ApplicationMember
  copy: ApplicationCopy
  pending: boolean
  onStage: (stage: Stage) => void
  onRemove: () => void
}

function MemberRow({ member, copy, pending, onStage, onRemove }: MemberRowProps): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap" data-member-id={member.pipeline.id}>
      <Stack gap={0} flex={1} miw={0}>
        <Text size="sm" truncate>
          {member.pipeline.name}
        </Text>
        <Text size="xs" c="dimmed" truncate>
          {member.why}
        </Text>
      </Stack>
      <Select
        aria-label={copy.stage}
        data={stageOptions(copy)}
        value={member.stage}
        onChange={(value) => isStage(value) && onStage(value)}
        allowDeselect={false}
        disabled={pending}
        w={130}
      />
      <Button size="xs" variant="default" disabled={pending} onClick={onRemove}>
        {copy.remove}
      </Button>
    </Group>
  )
}

export function MembersModal({ detail, copy, onClose }: ApplicationModalProps): ReactNode {
  const [pipelineId, setPipelineId] = useState<string | null>(null)
  const [stage, setStage] = useState<Stage | null>(null)
  const pipelines = usePipelines()
  const edit = useEditApplication()
  const id = detail.summary.id
  const memberIds = new Set(detail.members.map((member) => member.pipeline.id))
  const options = (pipelines.data ?? [])
    .filter((pipeline) => !memberIds.has(pipeline.id))
    .map((pipeline) => ({ value: pipeline.id, label: `${pipeline.repository} / ${pipeline.name}` }))

  const add = (): void => {
    if (pipelineId !== null) {
      edit.mutate(
        { id, edit: { type: 'link', pipeline_id: pipelineId, stage } },
        {
          onSuccess: () => {
            setPipelineId(null)
            setStage(null)
          },
        }
      )
    }
  }

  return (
    <StandardModal opened onClose={onClose} size="lg" title={copy.members}>
      <Stack gap="md">
        {detail.members.map((member) => (
          <MemberRow
            key={member.pipeline.id}
            member={member}
            copy={copy}
            pending={edit.isPending}
            onStage={(next) =>
              edit.mutate({ id, edit: { type: 'stage', pipeline_id: member.pipeline.id, stage: next } })
            }
            onRemove={() => edit.mutate({ id, edit: { type: 'unlink', pipeline_id: member.pipeline.id } })}
          />
        ))}
        <Group gap="xs" align="flex-end" wrap="nowrap">
          <Select
            label={copy.add_member}
            data={options}
            value={pipelineId}
            onChange={setPipelineId}
            searchable
            flex={1}
            miw={0}
          />
          <Select
            label={copy.stage}
            data={stageOptions(copy)}
            value={stage}
            onChange={(value) => setStage(isStage(value) ? value : null)}
            clearable
            w={130}
          />
          <Button loading={edit.isPending} disabled={pipelineId === null} onClick={add}>
            {copy.add_member}
          </Button>
        </Group>
      </Stack>
    </StandardModal>
  )
}
