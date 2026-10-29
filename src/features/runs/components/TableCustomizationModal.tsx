import type { ReactNode } from 'react'

import { TableCustomizationSession, type TableCustomizationSessionProps } from './TableCustomizationSession'

interface TableCustomizationModalProps extends TableCustomizationSessionProps {
  sessionKey: number
}

export function TableCustomizationModal({ sessionKey, ...props }: TableCustomizationModalProps): ReactNode {
  return <TableCustomizationSession key={sessionKey} {...props} />
}
