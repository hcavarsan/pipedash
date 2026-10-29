import { Menu } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { iosOverlayClasses } from '../../theme/iosOverlays'

export function ActionSheetCancel(): ReactNode {
  const mobileShell = useMobileShell()

  if (!mobileShell) {
    return null
  }

  return <Menu.Item className={`${iosOverlayClasses.menuItem} ${iosOverlayClasses.menuCancel}`}>Cancel</Menu.Item>
}
