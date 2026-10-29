import { Group, Stack, TextInput } from '@mantine/core'
import { type ReactNode, useActionState, useRef, useState } from 'react'

import { useFormEnterNavigation } from '@/hooks/useFormEnterNavigation'
import { errorMessage } from '@/utils/errorDisplay'
import { isHttpUrl } from '@/utils/openExternal'

import { LiveStatus } from '../../../components/forms/LiveStatus'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { AccentText, FieldRow, RowTextInput } from '../../../components/ios/mobileParts'

interface ProxySettingsFormProps {
  savedProxy: string
  mobileShell: boolean
  pending: boolean
  onSave: (proxyUrl: string) => Promise<void>
}

interface ProxyActionState {
  error: string | null
}

const PROXY_DESCRIPTION =
  "Sends provider requests through this HTTP proxy, except Tekton and Flux. Leave it empty to connect directly or through HTTPS_PROXY. Proxies that need a password aren't supported."

function validateProxyUrl(value: string): string | null {
  if (!value) {
    return null
  }

  if (!isHttpUrl(value)) {
    return 'Enter a URL that starts with http:// or https://'
  }

  const { username, password } = new URL(value)

  return username || password ? 'Proxy URLs with credentials are not supported.' : null
}

export function ProxySettingsForm({ savedProxy, mobileShell, pending, onSave }: ProxySettingsFormProps): ReactNode {
  const formRef = useRef<HTMLFormElement>(null)
  useFormEnterNavigation(formRef)
  const [draft, setDraft] = useState<string | null>(null)
  const value = draft ?? savedProxy
  const trimmed = value.trim()
  const validationError = validateProxyUrl(trimmed)

  const [state, formAction, isPending] = useActionState(
    async (_previous: ProxyActionState, formData: FormData): Promise<ProxyActionState> => {
      const submitted = formData.get('proxy_url')
      const raw = typeof submitted === 'string' ? submitted : value
      const next = raw.trim()
      const invalid = validateProxyUrl(next)

      if (invalid) {
        return { error: invalid }
      }

      if (next === savedProxy) {
        return { error: null }
      }

      try {
        await onSave(next)
        setDraft((current) => (current === raw ? null : current))

        return { error: null }
      } catch (error) {
        return { error: errorMessage(error, "Couldn't save the proxy URL.") }
      }
    },
    { error: null }
  )

  const busy = pending || isPending
  const canSave = !busy && !validationError && trimmed !== savedProxy
  const error = validationError ?? state.error ?? undefined

  if (mobileShell) {
    return (
      <form ref={formRef} action={formAction} noValidate>
        <LiveStatus message={error ?? (busy ? 'Saving proxy URL' : '')} />
        <Stack gap="lg">
          <IosGroup footer={PROXY_DESCRIPTION}>
            <FieldRow title="Proxy URL">
              {(id) => (
                <RowTextInput
                  id={id}
                  name="proxy_url"
                  placeholder="None"
                  type="url"
                  inputMode="url"
                  autoComplete="off"
                  enterKeyHint="done"
                  value={value}
                  onChange={(event) => setDraft(event.currentTarget.value)}
                  error={error}
                  disabled={busy}
                />
              )}
            </FieldRow>
          </IosGroup>

          <IosGroup>
            <IosRow
              title={<AccentText>{busy ? 'Saving...' : 'Save proxy'}</AccentText>}
              disabled={!canSave}
              onClick={() => formRef.current?.requestSubmit()}
            />
          </IosGroup>
        </Stack>
      </form>
    )
  }

  return (
    <form ref={formRef} action={formAction} noValidate>
      <Stack gap="xs">
        <TextInput
          name="proxy_url"
          label="Proxy URL"
          description={PROXY_DESCRIPTION}
          placeholder="http://proxy.example.com:3128"
          type="url"
          inputMode="url"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          value={value}
          onChange={(event) => setDraft(event.currentTarget.value)}
          error={error}
          disabled={busy}
        />
        <Group justify="flex-end">
          <SubmitButton size="xs" disabled={!canSave}>
            Save proxy
          </SubmitButton>
        </Group>
      </Stack>
    </form>
  )
}
