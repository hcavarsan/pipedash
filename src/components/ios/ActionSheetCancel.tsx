import { Menu } from '@mantine/core'

import { useMobileShell } from '../../hooks/useIsMobile'
import { iosClasses } from '../../theme/ios'

export function ActionSheetCancel() {
  const mobileShell = useMobileShell()

  if (!mobileShell) {
    return null
  }

  return <Menu.Item className={iosClasses.menuCancel}>Cancel</Menu.Item>
}
