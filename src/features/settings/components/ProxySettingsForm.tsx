import { Group, Stack, TextInput } from '@mantine/core'
import { type ReactNode, useActionState, useRef, useState } from 'react'

import { useFormEnterNavigation } from '@/hooks/useFormEnterNavigation'
import { errorMessage } from '@/utils/errorDisplay'

import { LiveStatus } from '../../../components/forms/LiveStatus'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { AccentText, FieldRow, RowTextInput } from '../../../components/ios/mobileParts'
import { usePlugins } from '../../../queries/usePluginsQueries'

interface ProxySettingsFormProps {
  savedProxy: string
  mobileShell: boolean
  pending: boolean
  onSave: (proxyUrl: string) => Promise<void>
}

interface ProxyActionState {
  error: string | null
}

const proxyDescription = (direct: string[]): string =>
  [
    'Sends requests from providers that support HTTP proxies through this proxy.',
    direct.length > 0 ? `${direct.join(' and ')} always connect directly.` : null,
    "Leave it empty to connect directly or through HTTPS_PROXY. Proxies that need a password aren't supported.",
  ]
    .filter(Boolean)
    .join(' ')

export function ProxySettingsForm({ savedProxy, mobileShell, pending, onSave }: ProxySettingsFormProps): ReactNode {
  const formRef = useRef<HTMLFormElement>(null)
  useFormEnterNavigation(formRef)
  const [draft, setDraft] = useState<string | null>(null)
  const value = draft ?? savedProxy
  const trimmed = value.trim()
  const { data: plugins } = usePlugins()
  const description = proxyDescription(
    (plugins ?? []).filter((plugin) => !plugin.capabilities.uses_http_proxy).map((plugin) => plugin.name)
  )

  const [state, formAction, isPending] = useActionState(
    async (_previous: ProxyActionState, formData: FormData): Promise<ProxyActionState> => {
      const submitted = formData.get('proxy_url')
      const raw = typeof submitted === 'string' ? submitted : value
      const next = raw.trim()

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
  const canSave = !busy && trimmed !== savedProxy
  const error = state.error ?? undefined

  if (mobileShell) {
    const renderInput = (id: string): ReactNode => (
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
    )

    return (
      <form ref={formRef} action={formAction} noValidate>
        <LiveStatus message={error ?? (busy ? 'Saving proxy URL' : '')} />
        <Stack gap="lg">
          <IosGroup footer={description}>
            <FieldRow title="Proxy URL">{renderInput}</FieldRow>
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
          description={description}
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
