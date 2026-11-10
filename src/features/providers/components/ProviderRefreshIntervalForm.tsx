import { Button, Group, NumberInput, Text } from '@mantine/core'
import { type ReactNode, useId } from 'react'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosRow } from '../../../components/ios/IosList'
import { FieldRow, RowNumberInput } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNumberDraft } from '../../../hooks/useNumberDraft'
import { useUpdateProviderRefreshInterval } from '../../../queries/useProvidersQueries'
import type { ProviderSummary } from '../../../types/providers'

interface ProviderRefreshIntervalFormProps {
  provider: ProviderSummary
  onSaved: () => void
  onCancel: () => void
}

const MIN_INTERVAL = 5
const MAX_INTERVAL = 300

export const ProviderRefreshIntervalForm = ({
  provider,
  onSaved,
  onCancel,
}: ProviderRefreshIntervalFormProps): ReactNode => {
  const mobileShell = useMobileShell()
  const mutation = useUpdateProviderRefreshInterval()
  const errorId = useId()
  const draft = useNumberDraft({
    saved: provider.refresh_interval,
    min: MIN_INTERVAL,
    save: async (refreshInterval) => {
      await mutation.mutateAsync({ id: provider.id, refreshInterval })
      onSaved()
    },
  })

  const { error, pending } = draft
  const label = `Refresh interval in seconds for ${provider.name}`
  const status = error ?? (pending ? 'Saving refresh interval' : '')

  const handleSubmit = (event: { preventDefault: () => void }): void => {
    event.preventDefault()
    draft.commit()
  }

  if (mobileShell) {
    return (
      <form onSubmit={handleSubmit} noValidate style={{ display: 'contents' }}>
        <LiveStatus message={status} />
        <FieldRow title="Refresh interval" subtitle={error ? <span id={errorId}>{error}</span> : undefined}>
          {(id) => (
            <RowNumberInput
              id={id}
              name="refreshInterval"
              value={draft.value}
              onChange={draft.onChange}
              onBlur={draft.commit}
              min={MIN_INTERVAL}
              max={MAX_INTERVAL}
              step={5}
              suffix="s"
              clampBehavior="none"
              allowDecimal={false}
              required
              enterKeyHint="done"
              aria-label={label}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : undefined}
              disabled={pending}
            />
          )}
        </FieldRow>
        <IosRow title="Cancel" disabled={pending} onClick={onCancel} />
      </form>
    )
  }

  return (
    <form onSubmit={handleSubmit} noValidate>
      <LiveStatus message={status} />
      <Group gap="xs" align="flex-start" wrap="nowrap">
        <NumberInput
          name="refreshInterval"
          value={draft.value}
          onChange={draft.onChange}
          onBlur={draft.commit}
          error={error ? true : undefined}
          clampBehavior="none"
          allowDecimal={false}
          required
          min={MIN_INTERVAL}
          max={MAX_INTERVAL}
          step={5}
          aria-label={label}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errorId : undefined}
          disabled={pending}
          size="xs"
          w={80}
          suffix="s"
        />
        <Button type="button" size="compact-xs" variant="subtle" color="gray" onClick={onCancel} disabled={pending}>
          Cancel
        </Button>
      </Group>
      {error && (
        <Text id={errorId} size="xs" c="red">
          {error}
        </Text>
      )}
    </form>
  )
}
