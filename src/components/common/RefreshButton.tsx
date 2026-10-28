import { ActionIcon, type ActionIconProps } from '@mantine/core'
import { IconRefresh } from '@tabler/icons-react'
import type { ComponentPropsWithRef } from 'react'

type RefreshButtonProps = ActionIconProps &
  Omit<ComponentPropsWithRef<'button'>, keyof ActionIconProps | 'children'> & {
    pending: boolean
    iconSize: number
  }

export const RefreshButton = ({ pending, iconSize, onClick, ...props }: RefreshButtonProps) => (
  <ActionIcon
    {...props}
    data-refreshing={pending || undefined}
    aria-busy={pending}
    onClick={(event) => {
      if (!pending) {
        onClick?.(event)
      }
    }}
  >
    <IconRefresh size={iconSize} />
  </ActionIcon>
)
