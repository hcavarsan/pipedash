import { IconChevronRight } from '@tabler/icons-react'
import type { ReactNode } from 'react'

export const NAME_HINT = 'A friendly name to identify this provider'
export const ADVANCED_LABEL = 'Advanced settings'

export interface AdvancedSection {
  available: boolean
  shown: boolean
  onToggle: () => void
  content: ReactNode
}

export function AdvancedChevron({ shown }: { shown: boolean }): ReactNode {
  return (
    <IconChevronRight
      size={16}
      aria-hidden
      style={{ transform: shown ? 'rotate(90deg)' : undefined, transition: 'transform 150ms ease' }}
    />
  )
}
