import type { ReactNode } from 'react'
import { createPortal } from 'react-dom'

import { useMobileNavStore } from '../../stores/mobileNavStore'

export function NavBarActions({ children }: { children: ReactNode }): ReactNode {
  const slot = useMobileNavStore((s) => s.navSlot)

  return slot ? createPortal(children, slot) : null
}
