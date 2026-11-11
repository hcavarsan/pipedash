import type { ReactNode } from 'react'

import type { SheetDetent } from '../../lib/sheetDetents'
import type { ModalBaseProps } from '../../types/components'

export type ModalSize = 'md' | 'lg' | 'xl' | 'full'

export interface ModalAction {
  label: string
  onClick: () => void
  loading?: boolean | undefined
  disabled?: boolean | undefined
  destructive?: boolean | undefined
}

export interface StandardModalProps extends ModalBaseProps {
  title: ReactNode
  children: ReactNode
  footer?: ReactNode | ((requestClose: () => void) => ReactNode)
  loading?: boolean
  closable?: boolean
  size?: ModalSize
  contentPadding?: boolean
  disableScrollArea?: boolean
  fullHeight?: boolean
  primaryAction?: ModalAction
  backAction?: { label: string; onClick: () => void; minimal?: boolean }
  dirty?: boolean
  detents?: SheetDetent[]
}

export interface HeaderButtonProps {
  children: ReactNode
  ariaLabel?: string | undefined
  onClick: () => void
  strong?: boolean | undefined
  destructive?: boolean | undefined
  disabled?: boolean | undefined
  loading?: boolean | undefined
}
