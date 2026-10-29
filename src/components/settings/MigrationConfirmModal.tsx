import { Alert, Badge, Button, Card, Checkbox, Group, PasswordInput, Stack, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'

import { useFormEnterNavigation } from '../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../hooks/useIsMobile'
import type { MigrationOptions, MigrationPlan, MigrationStatsPreview } from '../../types'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'

import { FieldRow, SwitchRow } from './mobileParts'

interface MigrationConfirmModalProps {
  opened: boolean
  onClose: () => void
  migrationPlan: MigrationPlan
  stats?: MigrationStatsPreview | undefined
  onConfirm: (options: MigrationOptions) => Promise<void>
}

const PASSWORD_ATTRIBUTES = {
  autoComplete: 'current-password',
  autoCapitalize: 'off',
  autoCorrect: 'off',
  spellCheck: false,
  enterKeyHint: 'go',
} as const

export const MigrationConfirmModal = ({
  opened,
  onClose,
  migrationPlan,
  stats,
  onConfirm,
}: MigrationConfirmModalProps): ReactNode => {
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  useFormEnterNavigation(formRef)
  const passwordErrorId = useId()
  const [loading, setLoading] = useState(false)
  const [password, setPassword] = useState('')
  const [migrateData, setMigrateData] = useState(true)
  const [passwordBlurred, setPasswordBlurred] = useState(false)

  useEffect(() => {
    if (!opened) {
      setPassword('')
      setMigrateData(true)
      setPasswordBlurred(false)
    }
  }, [opened])

  const needsPassword = migrationPlan.migrate_tokens && migrateData
  const blocked = needsPassword && !password

  const handleConfirm = async () => {
    setPasswordBlurred(true)
    if (loading || blocked) {
      return
    }

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

  const modalFooter = (
    <Group justify="flex-end" gap="sm">
      <Button variant="default" size="sm" onClick={onClose} disabled={loading}>
        Cancel
      </Button>
      <Button size="sm" type="submit" form={formId} loading={loading} color="orange" disabled={mobileShell && blocked}>
        Proceed with migration
      </Button>
    </Group>
  )

  const shellBody = (
    <Stack gap="lg">
      <Text size="sm" c="dimmed" px="md">
        These configuration changes require migrating your data to the new storage backend. This process will move your
        providers, tokens, and cache to the new location.
      </Text>

      <IosGroup header="Migration summary">
        {summaryRows.map((row) => (
          <IosRow key={row.label} title={row.label} value={row.value ?? 'Unavailable'} />
        ))}
      </IosGroup>

      <IosGroup footer="Turn off to start with a fresh database. Existing data will remain in the old location.">
        <SwitchRow
          title="Migrate existing data"
          checked={migrateData}
          onChange={(e) => setMigrateData(e.currentTarget.checked)}
          disabled={loading}
        />
      </IosGroup>

      {needsPassword && (
        <IosGroup
          header="Vault password"
          footer={
            password ? (
              'Required to decrypt and migrate your encrypted tokens.'
            ) : (
              <Text span inherit c="red" id={passwordErrorId}>
                Password is required for token migration
              </Text>
            )
          }
        >
          <FieldRow title="Password">
            {(fieldId) => (
              <PasswordInput
                id={fieldId}
                variant="unstyled"
                aria-required
                aria-invalid={!password}
                aria-describedby={password ? undefined : passwordErrorId}
                placeholder="Required"
                value={password}
                onChange={(e) => setPassword(e.currentTarget.value)}
                disabled={loading}
                w="12em"
                styles={{ input: { minHeight: '2em', paddingInlineStart: 0, textAlign: 'right' } }}
                {...PASSWORD_ATTRIBUTES}
              />
            )}
          </FieldRow>
        </IosGroup>
      )}
    </Stack>
  )

  const desktopBody = (
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
          onBlur={() => setPasswordBlurred(true)}
          required
          disabled={loading}
          error={passwordBlurred && !password ? 'Password is required for token migration' : undefined}
          {...PASSWORD_ATTRIBUTES}
        />
      )}
    </Stack>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Migration required"
      loading={loading}
      footer={modalFooter}
      primaryAction={{ label: 'Migrate', onClick: handleConfirm, loading, disabled: blocked }}
      size="md"
    >
      <form
        id={formId}
        noValidate
        onSubmit={(event) => {
          event.preventDefault()
          void handleConfirm()
        }}
        ref={formRef}
      >
        {mobileShell ? shellBody : desktopBody}
      </form>
    </StandardModal>
  )
}
