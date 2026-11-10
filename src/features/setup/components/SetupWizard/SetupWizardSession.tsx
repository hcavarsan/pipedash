import { Stack } from '@mantine/core'
import { IconDatabase, IconListCheck, IconTransfer } from '@tabler/icons-react'
import { type ReactNode, useId, useRef, useState } from 'react'
import { ErrorState } from '../../../../components/common/ErrorState'
import { LoadingState } from '../../../../components/common/LoadingState'
import type { ModalAction } from '../../../../components/common/standardModalTypes'
import { DesktopFooter, ShellFooter, type WizardAction } from '../../../../components/forms/WizardFooter'
import { useIsMobile, useMobileShell } from '../../../../hooks/useIsMobile'
import type { StorageConfigResponse } from '../../../../types/storage'
import { isMobilePlatform } from '../../../../utils/platform'
import { sameJson } from '../../../../utils/sameJson'

import { ConfirmStep } from './ConfirmStep'
import { confirmSkipTransfer } from './SetupWizardActions'
import { SetupWizardLayout, type WizardForm } from './SetupWizardLayout'
import { StorageStep } from './StorageStep'
import { SuccessStep } from './SuccessStep'
import { initialDraft, toFieldErrors, toSetupDraft } from './setupDraft'
import { TransferStep } from './TransferStep'
import type { VaultPasswordStatus, WizardStep } from './types'
import { usePreparedSetup } from './usePreparedSetup'
import { useSetupSubmission } from './useSetupSubmission'

const STEP_META = {
  storage: { label: 'Storage', icon: <IconDatabase size={16} aria-hidden /> },
  transfer: { label: 'Transfer', icon: <IconTransfer size={16} aria-hidden /> },
  confirm: { label: 'Confirm', icon: <IconListCheck size={16} aria-hidden /> },
}

function saveLabelFor(needsMigration: boolean, transferData: boolean): string {
  if (!needsMigration) {
    return 'Save'
  }

  return transferData ? 'Save and transfer' : 'Switch without transferring'
}

const transferLabelFor = (skipsTransfer: boolean): string =>
  skipsTransfer ? 'Switch without transferring' : 'Continue'

function stepsFor(compact: boolean, transfer: boolean): WizardStep[] {
  const middle: WizardStep[] = transfer ? ['transfer'] : []

  return compact ? ['storage', ...middle, 'success'] : ['storage', ...middle, 'confirm', 'success']
}

interface SetupWizardSessionProps {
  opened: boolean
  currentConfig: StorageConfigResponse | null
  vaultStatus: VaultPasswordStatus
  defaultDataDir: string
  onComplete: () => void
  onClose?: (() => void) | undefined
}

export function SetupWizardSession(props: SetupWizardSessionProps): ReactNode {
  const { opened, onComplete, onClose } = props
  const [initial] = useState({
    currentConfig: props.currentConfig,
    vaultStatus: props.vaultStatus,
    defaultDataDir: props.defaultDataDir,
  })
  const { currentConfig, vaultStatus, defaultDataDir } = initial
  const [draft, setDraft] = useState(() => initialDraft(currentConfig, defaultDataDir))
  const [firstDraft] = useState(() => draft)
  const [step, setStep] = useState<WizardStep>('storage')
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const compact = mobileShell && isMobilePlatform()

  const vaultPasswordFromEnv = vaultStatus.is_set
  const preparation = usePreparedSetup(draft, !currentConfig)
  const { prepared } = preparation
  const needsMigration = prepared?.needs_migration ?? false
  const reconfigured = Boolean(currentConfig)
  const showTransferStep = compact ? needsMigration : reconfigured

  const wizardSteps = stepsFor(compact, showTransferStep)
  const stepIndex = Math.max(wizardSteps.indexOf(step), 0)
  const isSuccessStep = step === 'success'
  const stepperSteps = wizardSteps.flatMap((name) => (name === 'success' ? [] : [STEP_META[name]]))
  const validation = toFieldErrors(prepared?.errors ?? {})
  const canProceedFromStorage =
    preparation.current && prepared !== undefined && Object.keys(prepared.errors).length === 0
  const setupDraft = toSetupDraft(draft)
  const dirty = !isSuccessStep && !sameJson(setupDraft, toSetupDraft(firstDraft))
  const saveLabel = saveLabelFor(needsMigration, draft.transferData)
  const skipsTransfer = !draft.transferData && needsMigration
  const transferLabel = compact ? saveLabel : transferLabelFor(skipsTransfer)
  const submits =
    (step === 'storage' && compact && !needsMigration && !currentConfig) ||
    (step === 'transfer' && compact) ||
    step === 'confirm'

  const [actionState, formAction, isPending] = useSetupSubmission({
    draft,
    firstRun: !currentConfig,
    step,
    submits,
    compact,
    onComplete,
  })
  const [handledState, setHandledState] = useState(actionState)

  if (handledState !== actionState) {
    setHandledState(actionState)

    if (actionState.saved) {
      setStep('success')
    }
  }

  const goTo = (name: WizardStep): void => setStep(wizardSteps.includes(name) ? name : 'storage')
  const goNext = (): void => setStep(wizardSteps[stepIndex + 1] ?? 'success')
  const goBack = (): void => setStep(wizardSteps[Math.max(stepIndex - 1, 0)] ?? 'storage')

  const advanceFromStorage = (): void => {
    if (canProceedFromStorage) {
      goNext()
    }
  }

  let wizardForm: WizardForm | null = null

  if (submits) {
    wizardForm = { id: formId, ref: formRef, action: formAction }
  } else if (step === 'storage') {
    wizardForm = { id: formId, ref: formRef, action: advanceFromStorage }
  }

  const afterTransfer = (): void => {
    if (compact) {
      formRef.current?.requestSubmit()
    } else {
      goTo('confirm')
    }
  }

  const continueFromTransfer = (): void => {
    if (mobileShell && !draft.transferData && needsMigration) {
      confirmSkipTransfer(afterTransfer)
    } else {
      afterTransfer()
    }
  }

  const primary = ((): WizardAction | undefined => {
    if (step === 'storage') {
      if (compact && !needsMigration) {
        return currentConfig ? undefined : { label: 'Get started', submit: true, disabled: !canProceedFromStorage }
      }

      return { label: 'Continue', submit: true, disabled: !canProceedFromStorage }
    }

    if (step === 'transfer') {
      return { label: transferLabel, onClick: continueFromTransfer }
    }

    return step === 'confirm' ? { label: saveLabel, submit: true } : undefined
  })()
  const busyPrimary = primary && { ...primary, loading: isPending }

  const canGoBack = stepIndex > 0 && !isSuccessStep
  const failure = actionState.errorStep === step && sameJson(actionState.submitted, setupDraft) ? actionState : null
  const error = failure?.error ?? null
  const preparationStatus = ((): ReactNode => {
    if (preparation.error) {
      return (
        <ErrorState
          title="Couldn't check the storage settings"
          error={preparation.error}
          onRetry={preparation.refetch}
          retrying={preparation.isFetching}
        />
      )
    }

    return prepared === undefined ? <LoadingState variant="compact" message="Checking storage settings…" /> : null
  })()
  const desktopFooter = (requestClose: () => void): ReactNode => (
    <DesktopFooter
      primary={busyPrimary}
      formId={formId}
      busy={isPending}
      onCancel={step === 'storage' && onClose ? requestClose : undefined}
      onBack={canGoBack ? goBack : undefined}
      fullWidth={isMobile}
    />
  )
  const shellPrimary: ModalAction | undefined = primary
    ? {
        label: primary.label,
        onClick: primary.submit ? () => formRef.current?.requestSubmit() : (primary.onClick ?? goNext),
        loading: isPending,
        disabled: primary.disabled === true,
      }
    : undefined

  const content = ((): ReactNode => {
    switch (step) {
      case 'transfer':
        return <TransferStep state={draft} setState={setDraft} error={error} />
      case 'confirm':
        return (
          <ConfirmStep
            state={draft}
            showTransferStep={showTransferStep}
            vaultPasswordFromEnv={vaultPasswordFromEnv}
            isMobile={isMobile}
            error={error}
            vaultStatus={vaultStatus}
          />
        )
      case 'success':
        return (
          <SuccessStep
            state={draft}
            migrationStats={actionState.migrationStats}
            vaultStatus={vaultStatus}
            vaultPasswordFromEnv={vaultPasswordFromEnv}
            onComplete={onComplete}
          />
        )
      default:
        return (
          <Stack gap="md">
            {preparationStatus}
            <StorageStep
              state={draft}
              setState={setDraft}
              currentConfig={currentConfig}
              vaultStatus={vaultStatus}
              error={error}
              fieldErrors={failure?.fieldErrors ?? {}}
              validation={validation}
              passwordWarnings={prepared?.password_warnings ?? []}
            />
          </Stack>
        )
    }
  })()

  return (
    <SetupWizardLayout
      opened={opened}
      settings={Boolean(onClose)}
      closable={isSuccessStep || Boolean(onClose)}
      onClose={isSuccessStep ? onComplete : (onClose ?? onComplete)}
      title={showTransferStep ? 'Storage settings' : 'Welcome to Pipedash'}
      onboardingTitle={isSuccessStep ? undefined : 'Welcome to Pipedash'}
      busy={isPending}
      stepper={isSuccessStep ? null : { steps: stepperSteps, active: stepIndex }}
      region={{
        stepKey: step,
        position: isSuccessStep ? 'Finished' : `Step ${stepIndex + 1} of ${stepperSteps.length}`,
        title: isSuccessStep ? 'Setup complete' : (stepperSteps[stepIndex]?.label ?? ''),
      }}
      form={wizardForm}
      dirty={dirty}
      footer={isSuccessStep ? null : desktopFooter}
      shellFooter={
        primary || canGoBack ? (
          <ShellFooter primary={busyPrimary} busy={isPending} onBack={canGoBack ? goBack : undefined} />
        ) : null
      }
      primaryAction={shellPrimary}
      onBack={canGoBack ? goBack : undefined}
    >
      {content}
    </SetupWizardLayout>
  )
}
