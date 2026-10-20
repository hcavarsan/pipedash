import { Text, type TextProps, Tooltip } from '@mantine/core'

interface TruncatedTextProps extends TextProps {
  children: string
  maxWidth?: number | string
}

export const TruncatedText = ({ children, maxWidth = '100%', ...textProps }: TruncatedTextProps) => {
  return (
    <Tooltip label={children} openDelay={500} withinPortal>
      <Text truncate maw={maxWidth} {...textProps}>
        {children}
      </Text>
    </Tooltip>
  )
}
