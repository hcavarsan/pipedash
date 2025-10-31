import { useState } from 'react'

import { ActionIcon, Tooltip } from '@mantine/core'
import { IconCheck, IconCopy } from '@tabler/icons-react'

interface CopyButtonProps {
  value: string;
  label?: string;
  size?: 'xs' | 'sm' | 'md' | 'lg';
}

export const CopyButton = ({ value, label = 'Copy', size = 'sm' }: CopyButtonProps) => {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (error) {
      console.error('Failed to copy:', error)
    }
  }

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
