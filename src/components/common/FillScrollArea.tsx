import { ScrollArea, type ScrollAreaProps } from '@mantine/core'
import type { ReactNode } from 'react'

const fill = { flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' } as const
const viewport = { flex: 1, minHeight: 0, height: 'auto' } as const

export function FillScrollArea({ style, ...props }: ScrollAreaProps): ReactNode {
  return <ScrollArea {...props} style={[fill, style]} styles={{ viewport }} />
}
