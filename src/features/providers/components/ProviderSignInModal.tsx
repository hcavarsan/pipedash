import { Anchor, Button, Group, Loader, Stack, Text } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useEffect, useRef, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { type ProviderSignInSession, useProviderStore } from '../../../stores/providerStore'
import type { SignInStatus } from '../../../types/providers'
import { errorMessage } from '../../../utils/errorDisplay'
import { openExternal } from '../../../utils/openExternal'
import { cancelProviderSignIn, reconnectProviderSignIn, useProviderSignIn } from '../queries/useProviderSignIn'
import { useCompleteSignIn, useSignInStatus } from '../queries/useSignInQueries'
import { SignInPasteControls } from './SignInPasteControls'

interface FailureInput {
  sessionError: string | null | undefined
  status: SignInStatus | undefined
  terminal: boolean
  statusError: unknown
  openError: string | null
}

function resolveFailure({ sessionError, status, terminal, statusError, openError }: FailureInput) {
  if (sessionError !== null && sessionError !== undefined) {
    return sessionError
  }

  if (terminal) {
    return status?.state === 'failed' ? status.message : 'The sign-in was cancelled or timed out. Start it again.'
  }

  if (statusError) {
    return errorMessage(statusError, "Couldn't check the sign-in")
  }

  return openError
}

interface WaitingActionsProps {
  showCheck: boolean
  checking: boolean
  showPasteLink: boolean
  onOpenAgain: () => void
  onCheck: () => void
  onShowPaste: () => void
}

function WaitingActions(p: WaitingActionsProps): ReactNode {
  return (
    <>
      <Group gap="xs">
        <Button variant="default" onClick={p.onOpenAgain}>
          Open the sign-in page again
        </Button>
        {p.showCheck && (
          <Button variant="subtle" onClick={p.onCheck} loading={p.checking}>
            Check again
          </Button>
        )}
      </Group>
      {p.showPasteLink && (
        <Anchor component="button" type="button" size="sm" ta="left" onClick={p.onShowPaste}>
          Browser didn't return to Pipedash?
        </Anchor>
      )}
    </>
  )
}

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
  const failure = resolveFailure({ sessionError: session.error, status, terminal, statusError, openError })

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

  let phaseLine: ReactNode = null

  if (session.phase === 'starting' || saving || waiting) {
    let phaseText = `Waiting for you to sign in with ${session.flow?.label ?? 'SSO'}…`

    if (session.phase === 'starting') {
      phaseText = 'Opening the sign-in page…'
    } else if (saving) {
      phaseText = 'Saving your sign-in and reconnecting the provider…'
    }

    phaseLine = (
      <Group gap="xs" wrap="nowrap">
        <Loader size="xs" aria-hidden />
        <Text size="sm">{phaseText}</Text>
      </Group>
    )
  } else if (saveFailed) {
    phaseLine = <Text size="sm">You are signed in. Retry saving to reconnect the provider.</Text>
  }

  let retryButton: ReactNode = null

  if (saveFailed) {
    retryButton = <Button onClick={() => reconnectProviderSignIn(queryClient, session.requestId)}>Retry saving</Button>
  } else if (session.phase === 'failed' || terminal) {
    retryButton = <Button onClick={retry}>Sign in again</Button>
  }

  return (
    <StandardModal opened onClose={() => cancelProviderSignIn()} title="Sign in" size="md" closable={!saving}>
      <Stack gap="md">
        <div role="status" aria-live="polite">
          {phaseLine}
        </div>
        {failure && (
          <Text size="sm" c="red" role="alert">
            {failure}
          </Text>
        )}
        {waiting && (
          <WaitingActions
            showCheck={Boolean(statusError)}
            checking={isFetching}
            showPasteLink={!pasteVisible}
            onOpenAgain={openAgain}
            onCheck={() => void refetch()}
            onShowPaste={() => setPasteOpen(true)}
          />
        )}
        {pasteVisible && (
          <SignInPasteControls
            value={pasted}
            error={complete.error ? errorMessage(complete.error, "Couldn't finish the sign-in") : undefined}
            pending={complete.isPending}
            onChange={setPasted}
            onSubmit={finish}
          />
        )}
        <Group justify="flex-end">
          <Button variant="subtle" onClick={() => cancelProviderSignIn()} disabled={saving}>
            Cancel
          </Button>
          {retryButton}
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
