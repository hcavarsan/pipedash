import { Alert, Badge, Button, Card, Checkbox, Group, PasswordInput, Stack, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { FieldRow, SwitchRow } from '../../../components/ios/mobileParts'
import { useFormEnterNavigation } from '../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { MigrationOptions, MigrationPlan, MigrationStatsPreview } from '../../../types/storage'
import { sameJson } from '../../../utils/sameJson'

interface MigrationConfirmModalProps {
  onClose: () => void
  migrationPlan: MigrationPlan
  stats?: MigrationStatsPreview | undefined
  onConfirm: (options: MigrationOptions) => Promise<void>
}

interface MigrationActionState {
  error: string | null
  submitted: { password: string; migrateData: boolean } | null
}

const PASSWORD_ATTRIBUTES = {
  autoComplete: 'current-password',
  autoCapitalize: 'off',
  autoCorrect: 'off',
  spellCheck: false,
  enterKeyHint: 'go',
} as const

interface MigrationPasswordFieldProps {
  value: string
  onChange: (value: string) => void
  disabled: boolean
  id: string
  errorId: string
}

const MigrationPasswordField = ({ value, onChange, disabled, id, errorId }: MigrationPasswordFieldProps): ReactNode => (
  <PasswordInput
    id={id}
    name="password"
    variant="unstyled"
    aria-required
    aria-invalid={!value}
    aria-describedby={value ? undefined : errorId}
    placeholder="Required"
    value={value}
    onChange={(e) => onChange(e.currentTarget.value)}
    disabled={disabled}
    w="12em"
    styles={{ input: { minHeight: '2em', paddingInlineStart: 0, textAlign: 'right' } }}
    {...PASSWORD_ATTRIBUTES}
  />
)

export const MigrationConfirmModal = ({
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
  const [password, setPassword] = useState('')
  const [migrateData, setMigrateData] = useState(true)
  const [passwordBlurred, setPasswordBlurred] = useState(false)

  const needsPassword = migrationPlan.migrate_tokens && migrateData
  const blocked = needsPassword && !password

  const [state, formAction, loading] = useActionState(
    async (_previous: MigrationActionState, formData: FormData): Promise<MigrationActionState> => {
      const submitted = formData.get('password')
      const tokenPassword = typeof submitted === 'string' ? submitted : password

      if (needsPassword && !tokenPassword) {
        setPasswordBlurred(true)

        return { error: null, submitted: null }
      }

      try {
        await onConfirm({
          migrate_tokens: migrateData,
          migrate_cache: migrateData,
          token_password: needsPassword && tokenPassword ? tokenPassword : undefined,
          dry_run: false,
        })

        return { error: null, submitted: null }
      } catch (error) {
        return {
          error: error instanceof Error ? error.message : 'Migration failed',
          submitted: { password: tokenPassword, migrateData },
        }
      }
    },
    { error: null, submitted: null }
  )
  const submittedError = sameJson(state.submitted, { password, migrateData }) ? state.error : null

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

  const passwordFooter = password ? (
    'Required to decrypt and migrate your encrypted tokens.'
  ) : (
    <Text span inherit c="red" id={passwordErrorId}>
      Password is required for token migration
    </Text>
  )

  const renderPasswordField = (fieldId: string): ReactNode => (
    <MigrationPasswordField
      id={fieldId}
      errorId={passwordErrorId}
      value={password}
      onChange={setPassword}
      disabled={loading}
    />
  )

  const passwordGroup = needsPassword && (
    <IosGroup header="Vault password" footer={passwordFooter}>
      <FieldRow title="Password">{renderPasswordField}</FieldRow>
    </IosGroup>
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

      {passwordGroup}
    </Stack>
  )

  const desktopSummaryRows = summaryRows.map((row) => (
    <Group key={row.label} justify="space-between">
      <Text size="sm" c="dimmed">
        {row.label}
      </Text>
      <Badge>{row.value ?? 'Unavailable'}</Badge>
    </Group>
  ))

  const desktopBody = (
    <Stack gap="md">
      <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="orange" title="Important">
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
          {desktopSummaryRows}
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
          name="password"
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
      opened
      onClose={onClose}
      title="Migration required"
      loading={loading}
      footer={modalFooter}
      primaryAction={{ label: 'Migrate', onClick: () => formRef.current?.requestSubmit(), loading, disabled: blocked }}
      size="md"
    >
      <form id={formId} noValidate action={formAction} ref={formRef}>
        <Stack gap="md">
          <FormErrorDisplay title="Couldn't save configuration" globalError={submittedError} />
          {mobileShell ? shellBody : desktopBody}
        </Stack>
      </form>
    </StandardModal>
  )
}
