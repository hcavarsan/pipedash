import { Button, Group, NumberInput, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { type ReactNode, useActionState, useId, useState } from 'react'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useUpdateProviderRefreshInterval } from '../../../queries/useProvidersQueries'
import type { ProviderSummary } from '../../../types'
import { errorMessage } from '../../../utils/errorDisplay'
import { LiveStatus } from '../../forms/LiveStatus'
import { SubmitButton } from '../../forms/SubmitButton'
import { IosRow } from '../../ios/IosList'
import { FieldRow, RowNumberInput } from '../mobileParts'

const INTERVAL_ERROR = 'Enter a whole number from 5 to 300 seconds.'

interface ProviderRefreshIntervalFormProps {
  provider: ProviderSummary
  onSaved: () => void
  onCancel: () => void
}

interface IntervalState {
  error: string | null
}

const isValidInterval = (value: number | string): boolean => {
  const parsed = Number(value)

  return value !== '' && Number.isInteger(parsed) && parsed >= 5 && parsed <= 300
}

export const ProviderRefreshIntervalForm = ({
  provider,
  onSaved,
  onCancel,
}: ProviderRefreshIntervalFormProps): ReactNode => {
  const mobileShell = useMobileShell()
  const mutation = useUpdateProviderRefreshInterval(mobileShell)
  const errorId = useId()
  const [draft, setDraft] = useState<number | string>(provider.refresh_interval)
  const [touched, setTouched] = useState(false)
  const invalid = !isValidInterval(draft)

  const [state, formAction, pending] = useActionState(
    async (previous: IntervalState): Promise<IntervalState> => {
      if (invalid) {
        setTouched(true)
        if (mobileShell) {
          notifications.show({ title: 'Invalid refresh interval', message: INTERVAL_ERROR, color: 'red' })
        }

        return previous
      }

      try {
        await mutation.mutateAsync({ id: provider.id, refreshInterval: Number(draft) })
        onSaved()

        return { error: null }
      } catch (error) {
        return { error: errorMessage(error, 'Try saving the interval again.') }
      }
    },
    { error: null }
  )

  const error = invalid ? (touched ? INTERVAL_ERROR : null) : state.error
  const label = `Refresh interval in seconds for ${provider.name}`
  const status = error ?? (pending ? 'Saving refresh interval' : '')

  if (mobileShell) {
    return (
      <form action={formAction} noValidate style={{ display: 'contents' }}>
        <LiveStatus message={status} />
        <FieldRow title="Refresh interval" subtitle={error ? <span id={errorId}>{error}</span> : undefined}>
          {(id) => (
            <RowNumberInput
              id={id}
              name="refreshInterval"
              value={draft}
              onChange={setDraft}
              onBlur={() => setTouched(true)}
              min={5}
              max={300}
              step={5}
              suffix="s"
              clampBehavior="none"
              allowDecimal={false}
              required
              aria-label={label}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : undefined}
              disabled={pending}
            />
          )}
        </FieldRow>
        <SubmitButton variant="subtle" fullWidth>
          Save
        </SubmitButton>
        <IosRow title="Cancel" disabled={pending} onClick={onCancel} />
      </form>
    )
  }

  return (
    <form action={formAction} noValidate>
      <LiveStatus message={status} />
      <Group gap="xs" align="flex-start" wrap="nowrap">
        <NumberInput
          name="refreshInterval"
          value={draft}
          onChange={setDraft}
          onBlur={() => setTouched(true)}
          error={error ? true : undefined}
          clampBehavior="none"
          allowDecimal={false}
          required
          min={5}
          max={300}
          step={5}
          aria-label={label}
          aria-describedby={error ? errorId : undefined}
          disabled={pending}
          size="xs"
          w={80}
          suffix="s"
        />
        <Button type="button" size="compact-xs" variant="subtle" color="gray" onClick={onCancel} disabled={pending}>
          Cancel
        </Button>
        <SubmitButton size="compact-xs" variant="light" color="gray">
          Save
        </SubmitButton>
      </Group>
      {error && (
        <Text id={errorId} size="xs" c="red">
          {error}
        </Text>
      )}
    </form>
  )
}
