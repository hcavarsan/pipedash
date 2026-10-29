import { ActionIcon, Menu } from '@mantine/core'
import { IconDotsCircleHorizontal } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { NavBarActions } from './NavBarActions'

export function NavBarMenu({ label, children }: { label: string; children: ReactNode }): ReactNode {
  return (
    <NavBarActions>
      <Menu>
        <Menu.Target>
          <ActionIcon color="blue" size={44} aria-label={label}>
            <IconDotsCircleHorizontal size={26} stroke={1.6} aria-hidden />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          {children}
          <ActionSheetCancel />
        </Menu.Dropdown>
      </Menu>
    </NavBarActions>
  )
}
