import { Text, type TextProps, Tooltip } from '@mantine/core'
import { type ReactNode, useEffect, useState } from 'react'

interface TruncatedTextProps extends TextProps {
  children: string
  maxWidth?: number | string
}

export const TruncatedText = ({ children, maxWidth = '100%', ...textProps }: TruncatedTextProps): ReactNode => {
  const [node, setNode] = useState<HTMLParagraphElement | null>(null)
  const [overflowing, setOverflowing] = useState(false)

  useEffect(() => {
    if (!node) {
      return undefined
    }

    const observer = new ResizeObserver(() => setOverflowing(node.scrollWidth > node.clientWidth))

    observer.observe(node)

    return () => observer.disconnect()
  }, [node])

  return (
    <Tooltip label={children} openDelay={500} withinPortal>
      <Text key={children} ref={setNode} truncate maw={maxWidth} tabIndex={overflowing ? 0 : undefined} {...textProps}>
        {children}
      </Text>
    </Tooltip>
  )
}
