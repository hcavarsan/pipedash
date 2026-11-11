import { Button } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { IconCheck, IconCopy } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { ApplicationCopy } from '../../../types/applications'

interface CopyTomlButtonProps {
  toml: string | null
  copy: ApplicationCopy
}

export function CopyTomlButton({ toml, copy }: CopyTomlButtonProps): ReactNode {
  const clipboard = useClipboard({ timeout: 2000 })

  if (toml === null) {
    return null
  }

  return (
    <Button
      size="xs"
      variant="default"
      leftSection={clipboard.copied ? <IconCheck size={14} aria-hidden /> : <IconCopy size={14} aria-hidden />}
      onClick={() => clipboard.copy(toml)}
    >
      {clipboard.copied ? copy.copied : copy.copy_toml}
    </Button>
  )
}
