import { Button, Card, CopyButton, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconCheck, IconCopy, IconEdit, IconFolderOpen } from '@tabler/icons-react'

import { isTauri, service } from '../../services'
import type { PipedashConfig, StoragePathsResponse } from '../../types'

interface StoragePathsDisplayProps {
  paths: StoragePathsResponse
  config?: PipedashConfig
  onEditConfig?: () => void
}

interface PathRowProps {
  label: string
  path: string
  showFolder?: boolean
  isConfigFile?: boolean
  onEdit?: () => void
}

const PathRow = ({ label, path, showFolder, isConfigFile, onEdit }: PathRowProps) => {
  const handleOpenFolder = async () => {
    if (isTauri()) {
      try {
        const folderPath = path.includes('.')
          ? path.substring(0, path.lastIndexOf('/'))
          : path

        await service.openUrl(`file://${folderPath}`)
      } catch (err) {
        console.error('Failed to open folder:', err)
      }
    }
  }

  return (
    <Group gap="xs" align="center" wrap="nowrap">
      <Text size="xs" c="dimmed" style={{ flexShrink: 0, width: '120px' }}>
        {label}
      </Text>

      <Tooltip label={path} position="top" withArrow multiline maw={400}>
        <Text
          size="xs"
          ff="monospace"
          style={{
            flex: 1,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            minWidth: 0,
            cursor: 'help'
          }}
        >
          {path}
        </Text>
      </Tooltip>

      <Group gap={4} wrap="nowrap" style={{ flexShrink: 0, width: showFolder ? '90px' : '60px', justifyContent: 'flex-end' }}>
        {isConfigFile && onEdit ? (
          <Tooltip label="Edit configuration file" position="top" withArrow>
            <Button
              size="compact-xs"
              variant="subtle"
              color="blue"
              onClick={onEdit}
              px={6}
            >
              <IconEdit size={14} />
            </Button>
          </Tooltip>
        ) : showFolder ? (
          <div style={{ width: '30px' }} />
        ) : null}

        <CopyButton value={`"${path}"`}>
          {({ copied, copy }) => (
            <Tooltip label={copied ? 'Copied!' : 'Copy full path'} position="top" withArrow>
              <Button
                size="compact-xs"
                variant="subtle"
                onClick={copy}
                px={6}
              >
                {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
              </Button>
            </Tooltip>
          )}
        </CopyButton>

        {showFolder && (
          <Tooltip label="Open in file manager" position="top" withArrow>
            <Button
              size="compact-xs"
              variant="subtle"
              onClick={handleOpenFolder}
              px={6}
            >
              <IconFolderOpen size={14} />
            </Button>
          </Tooltip>
        )}
      </Group>
    </Group>
  )
}

export const StoragePathsDisplay = ({ paths, config, onEditConfig }: StoragePathsDisplayProps) => {
  const isDesktop = isTauri()

  const isPostgresStorage = config?.storage?.backend === 'postgres'

  const isPostgresCache = config?.storage?.backend === 'postgres'

  return (
    <Card withBorder padding="md" radius="md">
      <Text size="sm" fw={600} mb="md">Storage Paths</Text>
      <Stack gap="sm">
        <PathRow
          label="Configuration"
          path={paths.config_file}
          showFolder={isDesktop}
          isConfigFile
          onEdit={onEditConfig}
        />

        {!isPostgresStorage && (
          <>
            <PathRow
              label="Database"
              path={paths.pipedash_db}
              showFolder={isDesktop}
            />
            <PathRow
              label="Metrics DB"
              path={paths.metrics_db}
              showFolder={isDesktop}
            />
          </>
        )}


        {(!isPostgresCache || !isPostgresStorage) && (
          <PathRow
            label="Data Directory"
            path={paths.data_dir}
            showFolder={isDesktop}
          />
        )}

        {!isPostgresCache && (
          <PathRow
            label="Cache Directory"
            path={paths.cache_dir}
            showFolder={isDesktop}
          />
        )}
      </Stack>
    </Card>
  )
}
