import { ActionIcon, Card, Group, Stack, Text, Tooltip, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import { IconCheck, IconCopy, IconEdit, IconFolderOpen } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { haptic } from '../../lib/haptics'
import { logger } from '../../lib/logger'
import { service } from '../../services'
import type { PipedashConfig, StoragePathsResponse } from '../../types'
import { formatErrorMessage } from '../../types/errors'
import { isDesktopTauri } from '../../utils/platform'
import { CopyButton } from '../atoms/CopyButton'
import { IosGroup, IosRow } from '../ios/IosList'

interface StoragePathsDisplayProps {
  paths: StoragePathsResponse
  config?: PipedashConfig
  onEditConfig?: () => void
}

interface PathRowProps {
  label: string
  path: string
  kind: 'file' | 'directory'
  canOpen: boolean
  onEdit?: (() => void) | undefined
}

const PathRow = ({ label, path, kind, canOpen, onEdit }: PathRowProps) => {
  const handleOpenFolder = async () => {
    const folderPath = kind === 'file' ? path.replace(/[\\/][^\\/]*$/, '') : path

    try {
      await service.openUrl(`file://${folderPath}`)
    } catch (err) {
      logger.error('StoragePathsDisplay', 'Failed to open folder', err)
      notifications.show({
        title: "Couldn't open folder",
        message: formatErrorMessage(err),
        color: 'red',
      })
    }
  }

  return (
    <Group gap="xs" align="center" wrap="nowrap">
      <Text size="xs" c="dimmed" w={120} style={{ flexShrink: 0 }}>
        {label}
      </Text>

      <Tooltip label={path} position="top" multiline maw={400}>
        <Text size="xs" ff="monospace" truncate="end" flex={1} miw={0}>
          {path}
        </Text>
      </Tooltip>

      <Group gap="3xs" wrap="nowrap" style={{ flexShrink: 0 }}>
        {onEdit && (
          <Tooltip label={`Edit ${label.toLowerCase()} file`} position="top">
            <ActionIcon size="sm" aria-label={`Edit ${label.toLowerCase()} file`} onClick={onEdit}>
              <IconEdit size={16} aria-hidden />
            </ActionIcon>
          </Tooltip>
        )}

        <CopyButton value={path} label={`Copy ${label.toLowerCase()} path`} />

        {canOpen && (
          <Tooltip label={`Open ${label.toLowerCase()} in file manager`} position="top">
            <ActionIcon size="sm" aria-label={`Open ${label.toLowerCase()} in file manager`} onClick={handleOpenFolder}>
              <IconFolderOpen size={16} aria-hidden />
            </ActionIcon>
          </Tooltip>
        )}
      </Group>
    </Group>
  )
}

interface CopyPathRowProps {
  label: string
  path: string
  copied: boolean
  onCopy: (label: string, path: string) => void
}

const CopyPathRow = ({ label, path, copied, onCopy }: CopyPathRowProps) => (
  <IosRow
    title={label}
    subtitle={path}
    onClick={() => onCopy(label, path)}
    accessory={
      copied ? <IconCheck size={18} color="var(--ios-green)" aria-hidden /> : <IconCopy size={18} aria-hidden />
    }
  />
)

export const StoragePathsDisplay = ({ paths, config, onEditConfig }: StoragePathsDisplayProps): ReactNode => {
  const canOpen = isDesktopTauri()
  const mobileShell = useMobileShell()
  const isPostgres = config?.storage?.backend === 'postgres'
  const { copy, copied } = useClipboard({ timeout: 2000 })
  const [copiedLabel, setCopiedLabel] = useState<string | null>(null)

  const copyPath = (label: string, path: string) => {
    copy(path)
    setCopiedLabel(label)
    haptic('success')
  }
  const row = (label: string, path: string) => (
    <CopyPathRow label={label} path={path} copied={copied && copiedLabel === label} onCopy={copyPath} />
  )

  if (mobileShell) {
    return (
      <>
        <IosGroup
          header="Storage paths"
          footer={
            <>
              Tap a path to copy it.
              <VisuallyHidden role="status">
                {copied && copiedLabel ? `Copied ${copiedLabel.toLowerCase()} path` : ''}
              </VisuallyHidden>
            </>
          }
        >
          {row('Configuration', paths.config_file)}
          {!isPostgres && (
            <>
              {row('Database', paths.pipedash_db)}
              {row('Metrics DB', paths.metrics_db)}
              {row('Data directory', paths.data_dir)}
              {row('Cache directory', paths.cache_dir)}
            </>
          )}
        </IosGroup>
        {onEditConfig && (
          <IosGroup>
            <IosRow title="Edit configuration file" icon={<IconEdit size={16} />} chevron onClick={onEditConfig} />
          </IosGroup>
        )}
      </>
    )
  }

  return (
    <Card padding="md">
      <Text size="sm" fw={600} mb="md">
        Storage paths
      </Text>
      <Stack gap="sm">
        <PathRow label="Configuration" path={paths.config_file} kind="file" canOpen={canOpen} onEdit={onEditConfig} />

        {!isPostgres && (
          <>
            <PathRow label="Database" path={paths.pipedash_db} kind="file" canOpen={canOpen} />
            <PathRow label="Metrics DB" path={paths.metrics_db} kind="file" canOpen={canOpen} />
            <PathRow label="Data directory" path={paths.data_dir} kind="directory" canOpen={canOpen} />
            <PathRow label="Cache directory" path={paths.cache_dir} kind="directory" canOpen={canOpen} />
          </>
        )}
      </Stack>
    </Card>
  )
}
