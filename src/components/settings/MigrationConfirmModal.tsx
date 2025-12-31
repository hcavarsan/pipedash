import { useState } from 'react'

import {
  Alert,
  Badge,
  Button,
  Card,
  Checkbox,
  Group,
  PasswordInput,
  Stack,
  Text,
} from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'

import type { MigrationOptions, MigrationPlan, MigrationStatsPreview } from '../../types'
import { StandardModal } from '../common/StandardModal'

interface MigrationConfirmModalProps {
  opened: boolean
  onClose: () => void
  migrationPlan: MigrationPlan
  stats?: MigrationStatsPreview
  onConfirm: (options: MigrationOptions) => Promise<void>
}

export const MigrationConfirmModal = ({
  opened,
  onClose,
  migrationPlan,
  stats,
  onConfirm,
}: MigrationConfirmModalProps) => {
  const [loading, setLoading] = useState(false)
  const [password, setPassword] = useState('')
  const [migrateData, setMigrateData] = useState(true)

  const needsPassword = migrationPlan.migrate_tokens && migrateData

  const handleConfirm = async () => {
    setLoading(true)
    try {
      await onConfirm({
        migrate_tokens: migrateData,
        migrate_cache: migrateData,
        token_password: needsPassword && password ? password : undefined,
        dry_run: false,
      })
    } finally {
      setLoading(false)
    }
  }

  const modalFooter = (
    <Group justify="flex-end" gap="sm">
      <Button variant="subtle" onClick={onClose} disabled={loading}>
        Cancel
      </Button>
      <Button
        onClick={handleConfirm}
        loading={loading}
        color="orange"
        disabled={needsPassword && !password}
      >
        Proceed with Migration
      </Button>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Migration Required"
      loading={loading}
      footer={modalFooter}
      disableAspectRatio
    >
      <Stack gap="md">
        <Alert icon={<IconAlertCircle size={16} />} color="orange" title="Important">
          <Text size="sm">
            These configuration changes require migrating your data to the new storage backend.
            {' '}
            This process will move your providers, tokens, and cache to the new location.
          </Text>
        </Alert>

        <Card withBorder padding="sm">
          <Stack gap="xs">
            <Text size="sm" fw={500}>
              Migration Summary
            </Text>
            <Group justify="space-between">
              <Text size="sm" c="dimmed">
                Providers
              </Text>
              <Badge>{stats?.providers_count || 0}</Badge>
            </Group>
            <Group justify="space-between">
              <Text size="sm" c="dimmed">
                Tokens
              </Text>
              <Badge>{stats?.tokens_count || 0}</Badge>
            </Group>
            <Group justify="space-between">
              <Text size="sm" c="dimmed">
                Cache Entries
              </Text>
              <Badge>{stats?.cache_entries_count || 0}</Badge>
            </Group>
            <Group justify="space-between">
              <Text size="sm" c="dimmed">
                Migration Steps
              </Text>
              <Badge>{migrationPlan.steps.length}</Badge>
            </Group>
          </Stack>
        </Card>

        <Checkbox
          label="Migrate existing data"
          description="Uncheck to start with a fresh database (existing data will remain in the old location)"
          checked={migrateData}
          onChange={(e) => setMigrateData(e.currentTarget.checked)}
          disabled={loading}
        />

        {needsPassword && (
          <PasswordInput
            label="Vault Password"
            description="Required to decrypt and migrate your encrypted tokens"
            placeholder="Enter your vault password"
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            required
            disabled={loading}
            error={needsPassword && !password ? 'Password is required for token migration' : undefined}
          />
        )}
      </Stack>
    </StandardModal>
  )
}
