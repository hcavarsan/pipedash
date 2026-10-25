import { ActionIcon, Tooltip, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCheck, IconCopy } from '@tabler/icons-react'

interface CopyButtonProps {
  value: string
  label: string
}

export const CopyButton = ({ value, label }: CopyButtonProps) => {
  const { copy, copied } = useClipboard({ timeout: 2000 })
  const currentLabel = copied ? 'Copied' : label

  return (
    <>
      <Tooltip label={currentLabel}>
        <ActionIcon
          variant="subtle"
          color={copied ? 'green' : 'gray'}
          size="sm"
          aria-label={currentLabel}
          onClick={(event) => {
            event.stopPropagation()
            copy(value)
          }}
        >
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
        </ActionIcon>
      </Tooltip>
      <VisuallyHidden role="status">{copied ? 'Copied' : ''}</VisuallyHidden>
    </>
  )
}
