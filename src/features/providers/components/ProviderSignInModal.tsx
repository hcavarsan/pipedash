import { Anchor, Button, Group, Loader, Stack, Text } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import type { ModalAction, StandardModalProps } from '../../../components/common/standardModalTypes'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { type ProviderSignInSession, useProviderStore } from '../../../stores/providerStore'
import type { SignInStatus } from '../../../types/providers'
import { errorMessage } from '../../../utils/errorDisplay'
import { openExternal } from '../../../utils/openExternal'
import { cancelProviderSignIn, reconnectProviderSignIn, useProviderSignIn } from '../queries/useProviderSignIn'
import { useCompleteSignIn, useSignInStatus } from '../queries/useSignInQueries'
import { ShellSignIn, type SignInViewProps } from './AddProviderModal/SignInFieldViews'
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
  const mobileShell = useMobileShell()
  const fieldId = useId()
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
  const pasteVisible = Boolean(waiting && (session.flow?.manual || pasteOpen))
  const failure = resolveFailure({ sessionError: session.error, status, terminal, statusError, openError })
  const pasteError = complete.error ? errorMessage(complete.error, "Couldn't finish the sign-in") : undefined

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

  const changePasted = (value: string) => {
    setPasted(value)
    complete.reset()
  }

  const finish = async () => {
    if (!session.flow || !pasted.trim() || completing.current) {
      return
    }

    completing.current = true

    try {
      await complete.mutateAsync({ id: session.flow.id, redirectUrl: pasted.trim() }).catch(() => undefined)
    } finally {
      completing.current = false
    }
  }

  let phaseText: string | null = null

  if (session.phase === 'starting') {
    phaseText = 'Opening the sign-in page…'
  } else if (saving) {
    phaseText = 'Saving your sign-in and reconnecting the provider…'
  } else if (waiting) {
    phaseText = `Waiting for you to sign in with ${session.flow?.label ?? 'SSO'}…`
  }

  let phaseLine: ReactNode = null

  if (phaseText !== null) {
    phaseLine = (
      <Group gap="xs" wrap="nowrap">
        <Loader size="xs" aria-hidden />
        <Text size="sm">{phaseText}</Text>
      </Group>
    )
  } else if (saveFailed) {
    phaseLine = <Text size="sm">You are signed in. Retry saving to reconnect the provider.</Text>
  }

  let retryAction: ModalAction | undefined

  if (saveFailed) {
    retryAction = { label: 'Retry saving', onClick: () => reconnectProviderSignIn(queryClient, session.requestId) }
  } else if (session.phase === 'failed' || terminal) {
    retryAction = { label: 'Sign in again', onClick: retry }
  }

  const pasteControls = (
    <SignInPasteControls
      shell={mobileShell}
      fieldId={`${fieldId}-paste`}
      value={pasted}
      error={pasteError}
      pending={complete.isPending}
      onChange={changePasted}
      onSubmit={() => void finish()}
    />
  )

  const shellView: SignInViewProps = {
    id: fieldId,
    phase: 'waiting',
    description: '',
    shownError: failure ?? undefined,
    invalid: false,
    signedIn: null,
    waitingText: phaseText ?? '',
    canOpenAgain: waiting && session.flow !== null,
    pasteLinkVisible: waiting && !pasteVisible,
    pasteVisible,
    pasteControls,
    pasteError,
    continueDisabled: !pasted.trim() || complete.isPending,
    blocked: false,
    starting: false,
    cancelling: false,
    onStart: () => Promise.resolve(retry()),
    onOpenAgain: openAgain,
    onShowPaste: () => setPasteOpen(true),
    onContinue: () => void finish(),
  }

  const footer = (
    <Group justify="flex-end">
      <Button variant="subtle" onClick={() => cancelProviderSignIn()} disabled={saving}>
        Cancel
      </Button>
      {retryAction && <Button onClick={retryAction.onClick}>{retryAction.label}</Button>}
    </Group>
  )

  const shellBody =
    phaseText === null ? (
      <Stack gap="md">
        {phaseLine}
        {failure && (
          <Text size="sm" c="red" role="alert">
            {failure}
          </Text>
        )}
      </Stack>
    ) : (
      <ShellSignIn {...shellView} />
    )

  const desktopBody = (
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
      {pasteVisible && pasteControls}
    </Stack>
  )

  let chrome: Pick<StandardModalProps, 'footer' | 'primaryAction'> = { footer }

  if (mobileShell) {
    chrome = retryAction ? { primaryAction: retryAction } : {}
  }

  return (
    <StandardModal
      opened
      onClose={() => cancelProviderSignIn()}
      title="Sign in"
      size="md"
      closable={!saving}
      {...chrome}
    >
      {mobileShell ? shellBody : desktopBody}
    </StandardModal>
  )
}

export function ProviderSignInModal(): ReactNode {
  const session = useProviderStore((state) => state.providerSignIn)

  useEffect(() => () => cancelProviderSignIn(true), [])

  return session ? <ProviderSignInContent key={session.requestId} session={session} /> : null
}
