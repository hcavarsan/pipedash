import { Text, type TextProps, Tooltip } from '@mantine/core'
import type { ReactNode } from 'react'

interface TruncatedTextProps extends TextProps {
  children: string
  maxWidth?: number | string
}

export const TruncatedText = ({ children, maxWidth = '100%', ...textProps }: TruncatedTextProps): ReactNode => {
  return (
    <Tooltip label={children} openDelay={500} withinPortal>
      <Text truncate maw={maxWidth} {...textProps}>
        {children}
      </Text>
    </Tooltip>
  )
}
