import { ActionIcon, Group, Loader, Menu, Tooltip } from '@mantine/core'
import { IconDots } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { PipelineRun } from '../../../types/domain'
import { useRunActionItems } from '../hooks/useRunActionItems'
import { useRunActions } from './RunActionsContext'

interface RunActionControlProps {
  run: PipelineRun
}

export function RunActionButtons({ run, withDetails }: RunActionControlProps & { withDetails: boolean }): ReactNode {
  const actions = useRunActionItems(run).filter((action) => withDetails || action.id !== 'details')

  return (
    <Group gap="xs" wrap="nowrap" justify="flex-end">
      {actions.map((action) => (
        <Tooltip key={action.id} label={action.label}>
          <ActionIcon
            type="button"
            variant="subtle"
            size="md"
            color="gray"
            disabled={action.disabled}
            loading={action.pending}
            aria-label={action.label}
            onClick={(event) => {
              event.stopPropagation()
              void action.execute()
            }}
          >
            {action.icon}
          </ActionIcon>
        </Tooltip>
      ))}
    </Group>
  )
}

export function RunActionMenu({ run }: RunActionControlProps): ReactNode {
  const { labels } = useRunActions()
  const actions = useRunActionItems(run)
  const busy = actions.some((action) => action.pending && action.id !== 'watch')

  return (
    <Menu position="bottom-end">
      <Menu.Target>
        <ActionIcon
          type="button"
          variant="subtle"
          color="gray"
          loading={busy}
          data-row-menu
          aria-label={`Actions for ${labels.run} #${run.run_number}`}
        >
          <IconDots size={22} aria-hidden />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        {actions.map((action) => (
          <Menu.Item
            key={action.id}
            {...(action.color === 'red' ? { color: 'red' } : {})}
            disabled={action.disabled}
            leftSection={action.pending && action.id === 'watch' ? <Loader size="xs" aria-hidden /> : action.icon}
            onClick={() => void action.execute()}
          >
            {action.label}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}
