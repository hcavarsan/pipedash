import { type ReactNode, useEffect, useRef, useState } from 'react'
import type { SignInStatus } from '../../../../types/providers'
import { displayErrorNotification, errorMessage } from '../../../../utils/errorDisplay'
import { openLink } from '../../../../utils/openExternal'
import { SIGN_IN_KEY } from '../../lib/credentials'
import { useCancelSignIn, useCompleteSignIn, useSignInStatus, useStartSignIn } from '../../queries/useSignInQueries'
import { SignInPasteControls } from '../SignInPasteControls'
import { DesktopSignIn, ShellSignIn, type SignInPhase, type SignInViewProps } from './SignInFieldViews'

const DESCRIPTION = 'Opens your identity provider in the browser. Pipedash renews the session before it expires.'
const FLOW_EXPIRED = 'The sign-in was cancelled or timed out. Start it again.'
const SESSION_EXPIRED = 'Your sign-in expired. Sign in again.'
const GENERIC_LABEL = 'SSO'

function failureMessage(status: SignInStatus | undefined, hasFlow: boolean): string | null {
  if (status?.state === 'failed') {
    return status.message
  }

  if (status?.state === 'expired') {
    return hasFlow ? FLOW_EXPIRED : SESSION_EXPIRED
  }

  return null
}

function signedInText(status: SignInStatus | undefined): string | null {
  if (status?.state !== 'completed') {
    return null
  }

  return status.account ? `Signed in as ${status.account}` : 'Signed in'
}

function signInPhase(waiting: boolean, checking: boolean, signedIn: string | null): SignInPhase {
  if (waiting) {
    return 'waiting'
  }

  if (checking) {
    return 'checking'
  }

  return signedIn ? 'signedIn' : 'idle'
}

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
  const { mutate: cancelFlow, mutateAsync: cancelOwnedFlow, isPending: cancelling } = useCancelSignIn()

  const flow = start.data?.id === reference ? start.data : null
  const waiting = status ? status.state === 'pending' : flow !== null
  const ownedFlowRef = useRef<string | null>(null)
  const unmountedRef = useRef(false)
  const completingRef = useRef(false)

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

  const handleCancel = async () => {
    const owned = ownedFlowRef.current

    if (owned) {
      try {
        await cancelOwnedFlow(owned)
      } catch (cancelError) {
        displayErrorNotification(cancelError, "Couldn't cancel sign-in")

        return
      }

      ownedFlowRef.current = null
    }

    resetPaste()
    start.reset()
    setPrevious(null)
    onChange(previous)
  }

  const changePasted = (value: string) => {
    setPasted(value)
    complete.reset()
  }

  const handleComplete = async () => {
    if (!reference || !pasted.trim() || completingRef.current) {
      return
    }

    completingRef.current = true

    try {
      await complete.mutateAsync({ id: reference, redirectUrl: pasted }).catch(() => undefined)
    } finally {
      completingRef.current = false
    }
  }

  const handleOpenAgain = () => {
    if (flow) {
      openLink(flow.authorize_url)
    }
  }

  const failure = failureMessage(status, flow !== null)
  const startError = start.error ? errorMessage(start.error, "Couldn't start the sign-in") : null
  const statusFailure = statusError ? errorMessage(statusError, "Couldn't check the sign-in") : null
  const shownError = startError ?? statusFailure ?? failure ?? error ?? undefined
  const checking = reference !== null && isLoading && flow === null
  const signedIn = signedInText(status)
  const waitingText = `Waiting for you to sign in with ${flow?.label ?? GENERIC_LABEL}…`
  const pasteVisible = waiting && flow !== null && (flow.manual || pasteOpen)
  const pasteLinkVisible = waiting && flow !== null && !pasteVisible
  const pasteError = complete.error ? errorMessage(complete.error, "Couldn't finish the sign-in") : undefined
  const blocked = missing.length > 0 || disabled || start.isPending
  const description =
    missing.length > 0 && !signedIn && !waiting
      ? `Fill in ${new Intl.ListFormat('en', { type: 'conjunction' }).format(missing)} to sign in.`
      : DESCRIPTION

  const view: SignInViewProps = {
    id,
    phase: signInPhase(waiting, checking, signedIn),
    description,
    shownError,
    invalid: Boolean(error),
    signedIn,
    waitingText,
    canOpenAgain: flow !== null,
    pasteLinkVisible,
    pasteVisible,
    pasteControls: (
      <SignInPasteControls
        shell={shell}
        value={pasted}
        error={pasteError}
        pending={complete.isPending}
        fieldId={`${id}-paste`}
        onChange={changePasted}
        onSubmit={() => void handleComplete()}
      />
    ),
    pasteError,
    continueDisabled: !pasted.trim() || complete.isPending,
    blocked,
    starting: start.isPending,
    cancelling,
    onStart: handleStart,
    onCancel: handleCancel,
    onOpenAgain: handleOpenAgain,
    onShowPaste: () => setPasteOpen(true),
    onContinue: () => void handleComplete(),
  }

  return shell ? <ShellSignIn {...view} /> : <DesktopSignIn {...view} />
}
