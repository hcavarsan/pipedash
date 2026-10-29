import { IconDatabase, IconListCheck, IconTransfer } from '@tabler/icons-react'
import { type ReactNode, useId, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { isTauri } from '../../../services'
import type { StorageConfigResponse } from '../../../types'
import { isMobilePlatform } from '../../../utils/platform'
import type { ModalAction } from '../../common/StandardModal'

import { ConfirmStep } from './ConfirmStep'
import { confirmSkipTransfer, DesktopFooter, ShellFooter, type WizardAction } from './SetupWizardActions'
import { SetupWizardLayout } from './SetupWizardLayout'
import { StorageStep } from './StorageStep'
import { SuccessStep } from './SuccessStep'
import { initialDraft, validateSetupDraft } from './setupDraft'
import { TransferStep } from './TransferStep'
import type { VaultPasswordStatus, WizardStep } from './types'
import { useSetupSubmission } from './useSetupSubmission'

const STEP_META = {
  storage: { label: 'Storage', icon: <IconDatabase size={16} aria-hidden /> },
  transfer: { label: 'Transfer', icon: <IconTransfer size={16} aria-hidden /> },
  confirm: { label: 'Confirm', icon: <IconListCheck size={16} aria-hidden /> },
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
  const [step, setStep] = useState<WizardStep>('storage')
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const compact = mobileShell && isMobilePlatform()

  const vaultPasswordFromEnv = vaultStatus.is_set
  const needsVaultPassword = !vaultPasswordFromEnv && !isMobilePlatform()
  const currentBackend = currentConfig?.config.storage.backend || 'sqlite'
  const currentDataDir = currentConfig?.config.storage.data_dir || defaultDataDir
  const isFromKeyring =
    Boolean(currentConfig) && isTauri() && !isMobilePlatform() && currentBackend === 'sqlite' && !vaultPasswordFromEnv
  const reconfigured = Boolean(currentConfig)
  const isDatabaseBackendChange = reconfigured && currentBackend !== draft.backend
  const isDataDirChange = reconfigured && currentDataDir !== draft.dataDir
  const needsMigration = isDatabaseBackendChange || isFromKeyring || isDataDirChange
  const showTransferStep = compact ? needsMigration : reconfigured

  const wizardSteps: WizardStep[] = compact
    ? showTransferStep
      ? ['storage', 'transfer', 'success']
      : ['storage', 'success']
    : showTransferStep
      ? ['storage', 'transfer', 'confirm', 'success']
      : ['storage', 'confirm', 'success']
  const stepIndex = Math.max(wizardSteps.indexOf(step), 0)
  const isSuccessStep = step === 'success'
  const stepperSteps = wizardSteps.flatMap((name) => (name === 'success' ? [] : [STEP_META[name]]))
  const canProceedFromStorage = Object.keys(validateSetupDraft(draft, needsVaultPassword)).length === 0
  const saveLabel = !needsMigration ? 'Save' : draft.transferData ? 'Save and transfer' : 'Switch without transferring'
  const transferLabel = compact
    ? saveLabel
    : !draft.transferData && needsMigration
      ? 'Switch without transferring'
      : 'Continue'
  const submits =
    (step === 'storage' && compact && !needsMigration && !currentConfig) ||
    (step === 'transfer' && compact) ||
    step === 'confirm'

  const [actionState, formAction, isPending] = useSetupSubmission({
    draft,
    currentConfig,
    defaultDataDir,
    step,
    submits,
    needsVaultPassword,
    needsMigration,
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

  const afterTransfer = (): void => {
    if (compact) {
      formRef.current?.requestSubmit()
    } else {
      goTo('confirm')
    }
  }

  const continueFromTransfer = (): void => {
    if (mobileShell && !draft.transferData && needsMigration) {
      confirmSkipTransfer(
        { databaseBackend: isDatabaseBackendChange, credentialStorage: isFromKeyring, dataDirectory: isDataDirChange },
        afterTransfer
      )
    } else {
      afterTransfer()
    }
  }

  const primary = ((): WizardAction | undefined => {
    if (step === 'storage') {
      if (compact && !needsMigration) {
        return currentConfig ? undefined : { label: 'Get started', submit: true, disabled: !canProceedFromStorage }
      }

      return { label: 'Continue', onClick: goNext, disabled: !canProceedFromStorage }
    }

    if (step === 'transfer') {
      return { label: transferLabel, onClick: continueFromTransfer }
    }

    return step === 'confirm' ? { label: saveLabel, submit: true } : undefined
  })()

  const canGoBack = stepIndex > 0 && !isSuccessStep
  const error = actionState.errorStep === step ? actionState.error : null
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
          <StorageStep
            state={draft}
            setState={setDraft}
            currentConfig={currentConfig}
            vaultStatus={vaultStatus}
            error={error}
            fieldErrors={actionState.errorStep === step ? actionState.fieldErrors : {}}
          />
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
      form={submits ? { id: formId, ref: formRef, action: formAction } : null}
      footer={
        isSuccessStep ? null : (
          <DesktopFooter
            primary={primary}
            formId={formId}
            isPending={isPending}
            onCancel={step === 'storage' ? onClose : undefined}
            onBack={canGoBack ? goBack : undefined}
            fullWidth={isMobile}
          />
        )
      }
      shellFooter={
        primary || canGoBack ? (
          <ShellFooter primary={primary} isPending={isPending} onBack={canGoBack ? goBack : undefined} />
        ) : null
      }
      primaryAction={shellPrimary}
      onBack={canGoBack ? goBack : undefined}
    >
      {content}
    </SetupWizardLayout>
  )
}
