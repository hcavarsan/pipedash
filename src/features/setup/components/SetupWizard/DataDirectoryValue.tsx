import { Group, Text, Tooltip } from '@mantine/core'
import type { ReactNode } from 'react'
import { CopyButton } from '../../../../components/atoms/CopyButton'

import { truncatePath } from './confirmSummary'

interface DataDirectoryValueProps {
  dataDir: string
  isMobile: boolean
}

export function DataDirectoryValue({ dataDir, isMobile }: DataDirectoryValueProps): ReactNode {
  return (
    <Group gap="3xs" wrap="nowrap" miw={0}>
      <Tooltip label={dataDir} multiline maw={300}>
        <Text size="xs" {...(dataDir ? { ff: 'monospace' } : {})} truncate maw={isMobile ? 140 : 180}>
          {truncatePath(dataDir) || 'Default'}
        </Text>
      </Tooltip>
      <CopyButton value={dataDir} label="Copy data directory path" />
    </Group>
  )
}
