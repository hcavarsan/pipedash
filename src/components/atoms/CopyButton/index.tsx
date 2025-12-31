import { ActionIcon, Tooltip } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCheck, IconCopy } from '@tabler/icons-react'

interface CopyButtonProps {
  value: string;
  label?: string;
  size?: 'xs' | 'sm' | 'md' | 'lg';
}

export const CopyButton = ({ value, label = 'Copy', size = 'sm' }: CopyButtonProps) => {
  const { copy, copied } = useClipboard({ timeout: 2000 })

  const handleCopy = () => copy(value)

  return (
    <Tooltip label={copied ? 'Copied!' : label}>
      <ActionIcon
        variant="subtle"
        color={copied ? 'green' : 'gray'}
        onClick={handleCopy}
        size={size}
      >
        {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
      </ActionIcon>
    </Tooltip>
  )
}
