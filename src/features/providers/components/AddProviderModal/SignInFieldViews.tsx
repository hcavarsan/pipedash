import { Anchor, Button, Group, Input, Loader, Stack, Text } from '@mantine/core'
import { IconCheck, IconLogin } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { PASTE_DESCRIPTION, PASTE_LABEL } from '../SignInPasteControls'
import { ShellField } from './ShellForm'

const LABEL = 'Single sign-on'
const PASTE_LINK = "Browser didn't return to Pipedash?"

export type SignInPhase = 'waiting' | 'checking' | 'signedIn' | 'idle'

export interface SignInViewProps {
  id: string
  phase: SignInPhase
  description: string
  shownError: string | undefined
  invalid: boolean
  signedIn: string | null
  waitingText: string
  canOpenAgain: boolean
  pasteLinkVisible: boolean
  pasteVisible: boolean
  pasteControls: ReactNode
  pasteError: string | undefined
  continueDisabled: boolean
  blocked: boolean
  starting: boolean
  onStart: () => Promise<void>
  onCancel: () => void
  onOpenAgain: () => void
  onShowPaste: () => void
  onContinue: () => void
}

function ShellBody(props: SignInViewProps): ReactNode {
  const { phase, signedIn, waitingText, canOpenAgain, pasteLinkVisible, blocked, starting, invalid } = props

  if (phase === 'waiting') {
    return (
      <>
        <IosRow title={waitingText} accessory={<Loader size="xs" aria-hidden />} />
        {canOpenAgain && <IosRow title="Open the sign-in page again" onClick={props.onOpenAgain} />}
        {pasteLinkVisible && <IosRow title={PASTE_LINK} onClick={props.onShowPaste} />}
        <IosRow title="Cancel" destructive onClick={props.onCancel} />
      </>
    )
  }

  if (phase === 'checking') {
    return <IosRow title="Checking sign-in…" accessory={<Loader size="xs" aria-hidden />} />
  }

  if (phase === 'signedIn') {
    return (
      <>
        <IosRow title={signedIn ?? ''} icon={<IconCheck size={18} aria-hidden />} />
        <IosRow title="Sign in again" onClick={props.onStart} disabled={blocked} />
      </>
    )
  }

  return (
    <IosRow
      title="Sign in with SSO"
      icon={<IconLogin size={18} aria-hidden />}
      accessory={starting ? <Loader size="xs" aria-hidden /> : undefined}
      onClick={props.onStart}
      disabled={blocked}
      invalid={invalid}
    />
  )
}

export function ShellSignIn(props: SignInViewProps): ReactNode {
  const { id, description, shownError, pasteVisible, pasteControls, pasteError, continueDisabled } = props

  return (
    <>
      <ShellField id={id} description={description} error={shownError}>
        <ShellBody {...props} />
      </ShellField>
      {pasteVisible && (
        <>
          <ShellField id={`${id}-paste`} label={PASTE_LABEL} description={PASTE_DESCRIPTION} error={pasteError}>
            {pasteControls}
          </ShellField>
          <IosGroup>
            <IosRow title="Continue" onClick={props.onContinue} disabled={continueDisabled} />
          </IosGroup>
        </>
      )}
    </>
  )
}

function DesktopBody(props: SignInViewProps): ReactNode {
  const { id, phase, signedIn, waitingText, canOpenAgain, pasteLinkVisible, pasteVisible, pasteControls } = props

  if (phase === 'waiting') {
    return (
      <>
        <Group gap="xs">
          <Loader size="xs" aria-hidden />
          <Text size="sm">{waitingText}</Text>
        </Group>
        <Group gap="xs">
          {canOpenAgain && (
            <Button id={id} variant="default" size="xs" onClick={props.onOpenAgain}>
              Open the sign-in page again
            </Button>
          )}
          <Button variant="subtle" size="xs" onClick={props.onCancel}>
            Cancel
          </Button>
        </Group>
        {pasteLinkVisible && (
          <Anchor component="button" type="button" size="xs" ta="left" onClick={props.onShowPaste}>
            {PASTE_LINK}
          </Anchor>
        )}
        {pasteVisible && pasteControls}
      </>
    )
  }

  if (phase === 'checking') {
    return (
      <Group gap="xs">
        <Loader size="xs" aria-hidden />
        <Text size="sm">Checking sign-in…</Text>
      </Group>
    )
  }

  if (phase === 'signedIn') {
    return (
      <Group gap="xs">
        <IconCheck size={16} aria-hidden />
        <Text size="sm">{signedIn}</Text>
        <Button id={id} variant="default" size="xs" onClick={props.onStart} disabled={props.blocked}>
          Sign in again
        </Button>
      </Group>
    )
  }

  return (
    <Group>
      <Button
        id={id}
        variant="default"
        leftSection={<IconLogin size={16} aria-hidden />}
        onClick={props.onStart}
        loading={props.starting}
        disabled={props.blocked}
        aria-invalid={props.invalid ? true : undefined}
      >
        Sign in with SSO
      </Button>
    </Group>
  )
}

export function DesktopSignIn(props: SignInViewProps): ReactNode {
  return (
    <Input.Wrapper
      id={props.id}
      labelElement="div"
      label={LABEL}
      description={props.description}
      error={props.shownError}
    >
      <Stack gap="xs" mt="xs">
        <DesktopBody {...props} />
      </Stack>
    </Input.Wrapper>
  )
}
