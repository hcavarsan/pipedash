import { ActionIcon, Button, Group, Popover, Stack, Text } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import type { ApplicationCopy, ApplicationEdit, FlowLink } from '../../../types/applications'
import { useAcceptSuggestions, useEditApplication } from '../queries/useApplicationQueries'

interface LinkPopoverProps {
  link: FlowLink
  copy: ApplicationCopy
  applicationId: number
  canEdit: boolean
}

interface LinkActionsProps {
  link: FlowLink
  copy: ApplicationCopy
  applicationId: number
  onDone: () => void
}

export const LINK_BUTTON_SIZE = 28

function LinkActions({ link, copy, applicationId, onDone }: LinkActionsProps): ReactNode {
  const editMutation = useEditApplication()
  const acceptMutation = useAcceptSuggestions()
  const pending = editMutation.isPending || acceptMutation.isPending
  const unlinkId = link.confidence === 'exact' ? link.unlink_pipeline_id : null
  const suggestionId = link.confidence === 'suggested' ? link.suggestion_id : null

  const edit = (change: ApplicationEdit): void => {
    editMutation.mutate({ id: applicationId, edit: change }, { onSuccess: onDone })
  }

  return (
    <Group gap="xs">
      {unlinkId !== null && (
        <Button
          size="xs"
          variant="default"
          loading={pending}
          onClick={() => edit({ type: 'unlink', pipeline_id: unlinkId })}
        >
          {copy.unlink}
        </Button>
      )}
      {suggestionId !== null && (
        <Button
          size="xs"
          loading={pending}
          onClick={() => acceptMutation.mutate([suggestionId], { onSuccess: onDone })}
        >
          {copy.accept}
        </Button>
      )}
      {suggestionId !== null && (
        <Button
          size="xs"
          variant="default"
          loading={pending}
          onClick={() => edit({ type: 'reject_suggestion', suggestion_id: suggestionId })}
        >
          {copy.reject}
        </Button>
      )}
    </Group>
  )
}

export function LinkPopover({ link, copy, applicationId, canEdit }: LinkPopoverProps): ReactNode {
  const [opened, setOpened] = useState(false)

  return (
    <Popover opened={opened} onChange={setOpened} withArrow position="bottom" width={280} shadow="md" returnFocus>
      <Popover.Target>
        <ActionIcon
          size={LINK_BUTTON_SIZE}
          radius="xl"
          variant="default"
          aria-label={copy.why_linked}
          onClick={() => setOpened((current) => !current)}
        >
          <IconInfoCircle size={16} aria-hidden />
        </ActionIcon>
      </Popover.Target>
      <Popover.Dropdown>
        <Stack gap="xs">
          <Text size="sm">{link.why}</Text>
          {link.hint !== null && (
            <Text size="xs" c="dimmed">
              {link.hint}
            </Text>
          )}
          {canEdit && (
            <LinkActions link={link} copy={copy} applicationId={applicationId} onDone={() => setOpened(false)} />
          )}
        </Stack>
      </Popover.Dropdown>
    </Popover>
  )
}
