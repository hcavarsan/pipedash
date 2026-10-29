import { ActionIcon, Tooltip, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCheck, IconCopy } from '@tabler/icons-react'
import type { ReactNode } from 'react'

interface CopyButtonProps {
  value: string
  label: string
}

export const CopyButton = ({ value, label }: CopyButtonProps): ReactNode => {
  const { copy, copied } = useClipboard({ timeout: 2000 })
  const currentLabel = copied ? 'Copied' : label

  return (
    <>
      <Tooltip label={currentLabel}>
        <ActionIcon
          color={copied ? 'green' : 'gray'}
          size="sm"
          aria-label={currentLabel}
          onClick={(event) => {
            event.stopPropagation()
            copy(value)
          }}
        >
          {copied ? <IconCheck size={16} aria-hidden /> : <IconCopy size={16} aria-hidden />}
        </ActionIcon>
      </Tooltip>
      <VisuallyHidden role="status">{copied ? 'Copied' : ''}</VisuallyHidden>
    </>
  )
}
