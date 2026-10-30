import { Anchor, Button, Group, Input, Loader, Stack, Text, TextInput } from '@mantine/core'
import { IconCheck, IconLogin } from '@tabler/icons-react'
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from 'react'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { errorMessage } from '../../../../utils/errorDisplay'
import { openExternal } from '../../../../utils/openExternal'
import { SIGN_IN_KEY } from '../../lib/credentials'
import { useCancelSignIn, useCompleteSignIn, useSignInStatus, useStartSignIn } from '../../queries/useSignInQueries'

import { ShellField } from './ShellForm'

const LABEL = 'Single sign-on'
const DESCRIPTION = 'Opens your identity provider in the browser. Pipedash renews the session before it expires.'
const PASTE_LABEL = 'Address of the page after sign-in'
const PASTE_DESCRIPTION =
  "After you sign in, the browser opens a page that can't load. Copy its address from the address bar and paste it here."
const PASTE_LINK = "Browser didn't return to Pipedash?"
const FLOW_EXPIRED = 'The sign-in was cancelled or timed out. Start it again.'
const SESSION_EXPIRED = 'Your sign-in expired. Sign in again.'
const GENERIC_LABEL = 'SSO'

interface SignInFieldProps {
  id: string
  providerType: string
  configValues: Record<string, string>
  missing: string[]
  error?: string | undefined
  disabled: boolean
  shell: boolean
  onChange: (reference: string | null) => void
}

export function SignInField({
  id,
  providerType,
  configValues,
  missing,
  error,
  disabled,
  shell,
  onChange,
}: SignInFieldProps): ReactNode {
  const [previous, setPrevious] = useState<string | null>(null)
  const [pasteOpen, setPasteOpen] = useState(false)
  const [pasted, setPasted] = useState('')
  const reference = configValues[SIGN_IN_KEY] ?? null
  const { data: status, isLoading, error: statusError } = useSignInStatus(reference)
  const start = useStartSignIn()
  const complete = useCompleteSignIn()
  const { mutate: cancelFlow } = useCancelSignIn()

  const flow = start.data?.id === reference ? start.data : null
  const waiting = status ? status.state === 'pending' : flow !== null
  const ownedFlowRef = useRef<string | null>(null)
  const unmountedRef = useRef(false)

  useEffect(() => {
    unmountedRef.current = false

    return () => {
      unmountedRef.current = true

      if (ownedFlowRef.current) {
        cancelFlow(ownedFlowRef.current)
      }
    }
  }, [cancelFlow])

  const resetPaste = () => {
    setPasteOpen(false)
    setPasted('')
    complete.reset()
  }

  const handleStart = async () => {
    const { [SIGN_IN_KEY]: current = null, ...config } = configValues
    const restore = status?.state === 'completed' ? current : previous

    resetPaste()
    const started = await start.mutateAsync({ providerType, config }).catch(() => null)

    if (!started) {
      return
    }

    if (unmountedRef.current) {
      cancelFlow(started.id)

      return
    }

    ownedFlowRef.current = started.id
    setPrevious(restore)
    onChange(started.id)
  }

  const handleCancel = () => {
    if (ownedFlowRef.current) {
      cancelFlow(ownedFlowRef.current)
      ownedFlowRef.current = null
    }

    resetPaste()
    start.reset()
    setPrevious(null)
    onChange(previous)
  }

  const handleComplete = () => {
    if (reference && pasted.trim()) {
      complete.mutate({ id: reference, redirectUrl: pasted })
    }
  }

  const handlePasteKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      handleComplete()
    }
  }

  const handleOpenAgain = () => {
    if (flow) {
      void openExternal(flow.authorize_url)
    }
  }

  const expiredMessage = flow ? FLOW_EXPIRED : SESSION_EXPIRED
  const failure = status?.state === 'failed' ? status.message : status?.state === 'expired' ? expiredMessage : null
  const startError = start.error ? errorMessage(start.error, "Couldn't start the sign-in") : null
  const statusFailure = statusError ? errorMessage(statusError, "Couldn't check the sign-in") : null
  const shownError = startError ?? statusFailure ?? failure ?? error ?? undefined
  const checking = reference !== null && isLoading && flow === null
  const signedIn =
    status?.state === 'completed' ? (status.account ? `Signed in as ${status.account}` : 'Signed in') : null
  const waitingText = `Waiting for you to sign in with ${flow?.label ?? GENERIC_LABEL}…`
  const pasteVisible = waiting && flow !== null && (flow.manual || pasteOpen)
  const pasteLinkVisible = waiting && flow !== null && !pasteVisible
  const pasteError = complete.error ? errorMessage(complete.error, "Couldn't finish the sign-in") : undefined
  const blocked = missing.length > 0 || disabled || start.isPending
  const description =
    missing.length > 0 && !signedIn && !waiting
      ? `Fill in ${new Intl.ListFormat('en', { type: 'conjunction' }).format(missing)} to sign in.`
      : DESCRIPTION

  const pasteInput = (
    <TextInput
      {...(shell ? { variant: 'unstyled' } : {})}
      label={shell ? undefined : PASTE_LABEL}
      description={shell ? undefined : PASTE_DESCRIPTION}
      aria-label={PASTE_LABEL}
      error={shell ? undefined : pasteError}
      value={pasted}
      onChange={(event) => setPasted(event.currentTarget.value)}
      onKeyDown={handlePasteKey}
      autoComplete="off"
      autoCapitalize="off"
      autoCorrect="off"
      spellCheck={false}
      inputMode="url"
      enterKeyHint="go"
    />
  )

  if (shell) {
    return (
      <>
        <ShellField id={id} description={description} error={shownError}>
          {waiting ? (
            <>
              <IosRow title={waitingText} accessory={<Loader size="xs" aria-hidden />} />
              {flow && <IosRow title="Open the sign-in page again" onClick={handleOpenAgain} />}
              {pasteLinkVisible && <IosRow title={PASTE_LINK} onClick={() => setPasteOpen(true)} />}
              <IosRow title="Cancel" destructive onClick={handleCancel} />
            </>
          ) : checking ? (
            <IosRow title="Checking sign-in…" accessory={<Loader size="xs" aria-hidden />} />
          ) : signedIn ? (
            <>
              <IosRow title={signedIn} icon={<IconCheck size={18} aria-hidden />} />
              <IosRow title="Sign in again" onClick={handleStart} disabled={blocked} />
            </>
          ) : (
            <IosRow
              title="Sign in with SSO"
              icon={<IconLogin size={18} aria-hidden />}
              accessory={start.isPending ? <Loader size="xs" aria-hidden /> : undefined}
              onClick={handleStart}
              disabled={blocked}
              invalid={Boolean(error)}
            />
          )}
        </ShellField>
        {pasteVisible && (
          <>
            <ShellField id={`${id}-paste`} label={PASTE_LABEL} description={PASTE_DESCRIPTION} error={pasteError}>
              {pasteInput}
            </ShellField>
            <IosGroup>
              <IosRow title="Continue" onClick={handleComplete} disabled={!pasted.trim() || complete.isPending} />
            </IosGroup>
          </>
        )}
      </>
    )
  }

  return (
    <Input.Wrapper id={id} labelElement="div" label={LABEL} description={description} error={shownError}>
      <Stack gap="xs" mt="xs">
        {waiting ? (
          <>
            <Group gap="xs">
              <Loader size="xs" aria-hidden />
              <Text size="sm">{waitingText}</Text>
            </Group>
            <Group gap="xs">
              {flow && (
                <Button id={id} variant="default" size="xs" onClick={handleOpenAgain}>
                  Open the sign-in page again
                </Button>
              )}
              <Button variant="subtle" size="xs" onClick={handleCancel}>
                Cancel
              </Button>
            </Group>
            {pasteLinkVisible && (
              <Anchor component="button" type="button" size="xs" ta="left" onClick={() => setPasteOpen(true)}>
                {PASTE_LINK}
              </Anchor>
            )}
            {pasteVisible && (
              <Stack gap="xs">
                {pasteInput}
                <Group>
                  <Button size="xs" onClick={handleComplete} loading={complete.isPending} disabled={!pasted.trim()}>
                    Continue
                  </Button>
                </Group>
              </Stack>
            )}
          </>
        ) : checking ? (
          <Group gap="xs">
            <Loader size="xs" aria-hidden />
            <Text size="sm">Checking sign-in…</Text>
          </Group>
        ) : signedIn ? (
          <Group gap="xs">
            <IconCheck size={16} aria-hidden />
            <Text size="sm">{signedIn}</Text>
            <Button id={id} variant="default" size="xs" onClick={handleStart} disabled={blocked}>
              Sign in again
            </Button>
          </Group>
        ) : (
          <Group>
            <Button
              id={id}
              variant="default"
              leftSection={<IconLogin size={16} aria-hidden />}
              onClick={handleStart}
              loading={start.isPending}
              disabled={blocked}
              aria-invalid={error ? true : undefined}
            >
              Sign in with SSO
            </Button>
          </Group>
        )}
      </Stack>
    </Input.Wrapper>
  )
}
