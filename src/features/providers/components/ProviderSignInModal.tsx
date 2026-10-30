import { Anchor, Button, Group, Loader, Stack, Text, TextInput } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { type ProviderSignInSession, useProviderStore } from '../../../stores/providerStore'
import { errorMessage } from '../../../utils/errorDisplay'
import { openExternal } from '../../../utils/openExternal'
import { cancelProviderSignIn, reconnectProviderSignIn, useProviderSignIn } from '../queries/useProviderSignIn'
import { useCompleteSignIn, useSignInStatus } from '../queries/useSignInQueries'

function ProviderSignInContent({ session }: { session: ProviderSignInSession }): ReactNode {
  const queryClient = useQueryClient()
  const signIn = useProviderSignIn()
  const { data: status, error: statusError, refetch, isFetching } = useSignInStatus(session.flow?.id ?? null)
  const complete = useCompleteSignIn()
  const [pasteOpen, setPasteOpen] = useState(false)
  const [pasted, setPasted] = useState('')
  const [openError, setOpenError] = useState<string | null>(null)
  const saving = session.phase === 'saving'
  const completing = useRef(false)
  const saveFailed = session.phase === 'save_failed'
  const terminal = status?.state === 'failed' || status?.state === 'expired'
  const waiting = session.phase === 'waiting' && !terminal && status?.state !== 'completed'
  const pasteVisible = waiting && (session.flow?.manual || pasteOpen)
  const failure =
    session.error ??
    (terminal
      ? status.state === 'failed'
        ? status.message
        : 'The sign-in was cancelled or timed out. Start it again.'
      : statusError
        ? errorMessage(statusError, "Couldn't check the sign-in")
        : openError)

  useEffect(() => {
    if (status?.state === 'completed' && session.phase === 'waiting') {
      reconnectProviderSignIn(queryClient, session.requestId)
    }
  }, [queryClient, session.phase, session.requestId, status?.state])

  const retry = () => {
    cancelProviderSignIn()
    signIn(session.providerId)
  }

  const openAgain = () => {
    if (session.flow) {
      setOpenError(null)
      void openExternal(session.flow.authorize_url).catch((error: unknown) => {
        setOpenError(errorMessage(error, "Couldn't open the sign-in page"))
      })
    }
  }

  const finish = () => {
    if (session.flow && pasted.trim() && !completing.current) {
      completing.current = true
      complete.mutate(
        { id: session.flow.id, redirectUrl: pasted.trim() },
        {
          onSettled: () => {
            completing.current = false
          },
        }
      )
    }
  }

  const pasteKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      finish()
    }
  }

  return (
    <StandardModal opened onClose={() => cancelProviderSignIn()} title="Sign in" size="md" closable={!saving}>
      <Stack gap="md">
        <div role="status" aria-live="polite">
          {session.phase === 'starting' || saving || waiting ? (
            <Group gap="xs" wrap="nowrap">
              <Loader size="xs" aria-hidden />
              <Text size="sm">
                {session.phase === 'starting'
                  ? 'Opening the sign-in page…'
                  : saving
                    ? 'Saving your sign-in and reconnecting the provider…'
                    : `Waiting for you to sign in with ${session.flow?.label ?? 'SSO'}…`}
              </Text>
            </Group>
          ) : saveFailed ? (
            <Text size="sm">You are signed in. Retry saving to reconnect the provider.</Text>
          ) : null}
        </div>
        {failure && (
          <Text size="sm" c="red" role="alert">
            {failure}
          </Text>
        )}
        {waiting && (
          <>
            <Group gap="xs">
              <Button variant="default" onClick={openAgain}>
                Open the sign-in page again
              </Button>
              {statusError && (
                <Button variant="subtle" onClick={() => void refetch()} loading={isFetching}>
                  Check again
                </Button>
              )}
            </Group>
            {!pasteVisible && (
              <Anchor component="button" type="button" size="sm" ta="left" onClick={() => setPasteOpen(true)}>
                Browser didn't return to Pipedash?
              </Anchor>
            )}
          </>
        )}
        {pasteVisible && (
          <Stack gap="xs">
            <TextInput
              label="Address of the page after sign-in"
              description="After you sign in, the browser opens a page that can't load. Copy its address from the address bar and paste it here."
              value={pasted}
              onChange={(event) => setPasted(event.currentTarget.value)}
              onKeyDown={pasteKey}
              error={complete.error ? errorMessage(complete.error, "Couldn't finish the sign-in") : undefined}
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              inputMode="url"
              enterKeyHint="go"
            />
            <Group>
              <Button onClick={finish} loading={complete.isPending} disabled={!pasted.trim()}>
                Continue
              </Button>
            </Group>
          </Stack>
        )}
        <Group justify="flex-end">
          <Button variant="subtle" onClick={() => cancelProviderSignIn()} disabled={saving}>
            Cancel
          </Button>
          {saveFailed ? (
            <Button onClick={() => reconnectProviderSignIn(queryClient, session.requestId)}>Retry saving</Button>
          ) : session.phase === 'failed' || terminal ? (
            <Button onClick={retry}>Sign in again</Button>
          ) : null}
        </Group>
      </Stack>
    </StandardModal>
  )
}

export function ProviderSignInModal(): ReactNode {
  const session = useProviderStore((state) => state.providerSignIn)

  useEffect(() => () => cancelProviderSignIn(true), [])

  return session ? <ProviderSignInContent key={session.requestId} session={session} /> : null
}
