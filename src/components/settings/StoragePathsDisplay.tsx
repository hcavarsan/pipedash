import { ActionIcon, Card, Group, Stack, Text, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconEdit, IconFolderOpen } from '@tabler/icons-react'

import { logger } from '../../lib/logger'
import { service } from '../../services'
import type { PipedashConfig, StoragePathsResponse } from '../../types'
import { isDesktopTauri } from '../../utils/platform'
import { CopyButton } from '../atoms/CopyButton'

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
  onEdit?: () => void
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
        message: err instanceof Error ? err.message : `Failed to open ${folderPath}`,
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

      <Group gap={4} wrap="nowrap" style={{ flexShrink: 0 }}>
        {onEdit && (
          <Tooltip label={`Edit ${label.toLowerCase()} file`} position="top">
            <ActionIcon
              variant="subtle"
              color="gray"
              size="sm"
              aria-label={`Edit ${label.toLowerCase()} file`}
              onClick={onEdit}
            >
              <IconEdit size={16} />
            </ActionIcon>
          </Tooltip>
        )}

        <CopyButton value={path} label={`Copy ${label.toLowerCase()} path`} />

        {canOpen && (
          <Tooltip label={`Open ${label.toLowerCase()} in file manager`} position="top">
            <ActionIcon
              variant="subtle"
              color="gray"
              size="sm"
              aria-label={`Open ${label.toLowerCase()} in file manager`}
              onClick={handleOpenFolder}
            >
              <IconFolderOpen size={16} />
            </ActionIcon>
          </Tooltip>
        )}
      </Group>
    </Group>
  )
}

export const StoragePathsDisplay = ({ paths, config, onEditConfig }: StoragePathsDisplayProps) => {
  const canOpen = isDesktopTauri()
  const isPostgres = config?.storage?.backend === 'postgres'

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
