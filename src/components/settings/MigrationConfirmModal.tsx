import { Alert, Badge, Button, Card, Checkbox, Group, PasswordInput, Stack, Switch, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { MigrationOptions, MigrationPlan, MigrationStatsPreview } from '../../types'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'

import { RowTextInput } from './mobileParts'

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
  const mobileShell = useMobileShell()
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

  const summaryRows = [
    { label: 'Providers', value: stats?.providers_count },
    { label: 'Tokens', value: stats?.tokens_count },
    { label: 'Cache entries', value: stats?.cache_entries_count },
    { label: 'Migration steps', value: migrationPlan.steps.length },
  ]

  if (mobileShell) {
    return (
      <StandardModal
        opened={opened}
        onClose={onClose}
        title="Migration required"
        loading={loading}
        footer={
          <Button
            fullWidth
            onClick={handleConfirm}
            loading={loading}
            color="orange"
            disabled={needsPassword && !password}
          >
            Proceed with migration
          </Button>
        }
        size="md"
      >
        <Stack gap="lg">
          <Text size="sm" c="dimmed" px="md">
            These configuration changes require migrating your data to the new storage backend. This process will move
            your providers, tokens, and cache to the new location.
          </Text>

          <IosGroup header="Migration summary">
            {summaryRows.map((row) => (
              <IosRow key={row.label} title={row.label} value={row.value ?? 'Unavailable'} />
            ))}
          </IosGroup>

          <IosGroup footer="Turn off to start with a fresh database. Existing data will remain in the old location.">
            <IosRow
              title="Migrate existing data"
              accessory={
                <Switch
                  aria-label="Migrate existing data"
                  checked={migrateData}
                  onChange={(e) => setMigrateData(e.currentTarget.checked)}
                  disabled={loading}
                />
              }
            />
          </IosGroup>

          {needsPassword && (
            <IosGroup
              header="Vault password"
              footer={
                password ? (
                  'Required to decrypt and migrate your encrypted tokens.'
                ) : (
                  <Text span inherit c="red">
                    Password is required for token migration
                  </Text>
                )
              }
            >
              <IosRow
                title="Password"
                accessory={
                  <RowTextInput
                    aria-label="Vault password"
                    type="password"
                    placeholder="Required"
                    value={password}
                    onChange={(e) => setPassword(e.currentTarget.value)}
                    disabled={loading}
                  />
                }
              />
            </IosGroup>
          )}
        </Stack>
      </StandardModal>
    )
  }

  const modalFooter = (
    <Group justify="flex-end" gap="sm">
      <Button variant="default" size="sm" onClick={onClose} disabled={loading}>
        Cancel
      </Button>
      <Button size="sm" onClick={handleConfirm} loading={loading} color="orange" disabled={needsPassword && !password}>
        Proceed with migration
      </Button>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Migration required"
      loading={loading}
      footer={modalFooter}
      size="md"
    >
      <Stack gap="md">
        <Alert icon={<IconAlertCircle size={16} />} color="orange" title="Important">
          <Text size="sm">
            These configuration changes require migrating your data to the new storage backend. This process will move
            your providers, tokens, and cache to the new location.
          </Text>
        </Alert>

        <Card padding="sm">
          <Stack gap="xs">
            <Text size="sm" fw={500}>
              Migration summary
            </Text>
            {summaryRows.map((row) => (
              <Group key={row.label} justify="space-between">
                <Text size="sm" c="dimmed">
                  {row.label}
                </Text>
                <Badge>{row.value ?? 'Unavailable'}</Badge>
              </Group>
            ))}
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
            label="Vault password"
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
