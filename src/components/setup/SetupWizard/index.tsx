import { Box, Button, Group, ScrollArea, Stack, Stepper, Text } from '@mantine/core'
import { IconCheck, IconDatabase, IconListCheck, IconTransfer } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import {
  useCreateInitialConfig,
  useDefaultDataDir,
  useExecuteMigration,
  usePlanStorageMigration,
  useSaveStorageConfig,
  useStorageConfig,
} from '../../../queries/useStorageQueries'
import { useUnlockVault, useVaultPasswordStatus } from '../../../queries/useVaultQueries'
import { isTauri } from '../../../services'
import type { MigrationResult, PipedashConfig } from '../../../types'
import { formatErrorMessage } from '../../../types/errors'
import { openConfirm } from '../../../utils/confirm'
import { isMobilePlatform } from '../../../utils/platform'
import { LoadingState } from '../../common/LoadingState'
import { type ModalAction, StandardModal } from '../../common/StandardModal'
import { StepRegion } from '../../forms/StepRegion'

import { ConfirmStep } from './ConfirmStep'
import { Onboarding, StepHeader } from './Onboarding'
import { isPostgresUrl, StorageStep } from './StorageStep'
import { SuccessStep } from './SuccessStep'
import { TransferStep } from './TransferStep'
import type { SetupState, SetupWizardProps, StorageBackend } from './types'

type WizardStep = 'storage' | 'transfer' | 'confirm' | 'success'

const STEP_META = {
  storage: { label: 'Storage', icon: <IconDatabase size={16} /> },
  transfer: { label: 'Transfer', icon: <IconTransfer size={16} /> },
  confirm: { label: 'Confirm', icon: <IconListCheck size={16} /> },
}

export const SetupWizard = ({ opened, onComplete, onClose }: SetupWizardProps) => {
  const { data: currentConfig, isLoading: loading } = useStorageConfig({ enabled: opened })
  const { data: vaultStatus, isLoading: checkingVault } = useVaultPasswordStatus({ enabled: opened })
  const { data: defaultDataDir = '' } = useDefaultDataDir({ enabled: opened })

  const createInitialConfigMutation = useCreateInitialConfig()
  const saveStorageConfigMutation = useSaveStorageConfig()
  const planMigrationMutation = usePlanStorageMigration()
  const executeMigrationMutation = useExecuteMigration()
  const unlockVaultMutation = useUnlockVault()

  const [step, setStep] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [migrationStats, setMigrationStats] = useState<MigrationResult['stats'] | null>(null)

  const [state, setState] = useState<SetupState>({
    backend: 'sqlite',
    dataDir: '',
    postgresUrl: '',
    vaultPassword: '',
    vaultPasswordConfirm: '',
    transferData: true,
  })

  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const compact = mobileShell && isMobilePlatform()

  const saving =
    createInitialConfigMutation.isPending ||
    saveStorageConfigMutation.isPending ||
    planMigrationMutation.isPending ||
    executeMigrationMutation.isPending ||
    unlockVaultMutation.isPending

  const getCurrentBackend = useCallback((): string => {
    if (!currentConfig) {
      return 'sqlite'
    }

    return currentConfig.config.storage.backend || 'sqlite'
  }, [currentConfig])

  const vaultPasswordFromEnv = vaultStatus?.is_set === true
  const needsVaultPassword = !vaultPasswordFromEnv && !isMobilePlatform()
  const isFromKeyring = useMemo(() => {
    if (!currentConfig || !isTauri() || isMobilePlatform()) {
      return false
    }
    const currentBackend = currentConfig.config.storage.backend || 'sqlite'

    return currentBackend === 'sqlite' && !vaultPasswordFromEnv
  }, [currentConfig, vaultPasswordFromEnv])

  const getCurrentDataDir = useCallback((): string => {
    if (!currentConfig) {
      return defaultDataDir
    }

    return currentConfig.config.storage.data_dir || defaultDataDir
  }, [currentConfig, defaultDataDir])

  const [reconfiguring, setReconfiguring] = useState<boolean | null>(null)
  const reconfigured = reconfiguring ?? Boolean(currentConfig)
  const isDatabaseBackendChange = reconfigured && getCurrentBackend() !== state.backend
  const isDataDirChange = reconfigured && getCurrentDataDir() !== state.dataDir

  const needsMigration = isDatabaseBackendChange || isFromKeyring || isDataDirChange
  const showTransferStep = compact ? needsMigration : reconfigured

  const wizardSteps = useMemo<WizardStep[]>(() => {
    if (compact) {
      return showTransferStep ? ['storage', 'transfer', 'success'] : ['storage', 'success']
    }

    return showTransferStep ? ['storage', 'transfer', 'confirm', 'success'] : ['storage', 'confirm', 'success']
  }, [compact, showTransferStep])
  const currentStep = wizardSteps[step] ?? 'storage'
  const isSuccessStep = currentStep === 'success'
  const goTo = useCallback((name: WizardStep) => setStep(Math.max(wizardSteps.indexOf(name), 0)), [wizardSteps])

  const stepperSteps = useMemo(
    () =>
      wizardSteps
        .filter((name): name is Exclude<WizardStep, 'success'> => name !== 'success')
        .map((name) => STEP_META[name]),
    [wizardSteps]
  )

  const passwordsMatch = state.vaultPassword === state.vaultPasswordConfirm
  const passwordValid = state.vaultPassword.length >= 8 && passwordsMatch

  useEffect(() => {
    if (!opened) {
      setStep(0)
      setState({
        backend: 'sqlite',
        dataDir: '',
        postgresUrl: '',
        vaultPassword: '',
        vaultPasswordConfirm: '',
        transferData: true,
      })
      setError(null)
      setMigrationStats(null)
      setReconfiguring(null)

      return
    }

    if (loading || !defaultDataDir) {
      return
    }

    setReconfiguring((prev) => prev ?? Boolean(currentConfig))

    if (currentConfig) {
      const currentBackend = currentConfig.config.storage.backend || 'sqlite'

      const effectiveDataDir = currentConfig.config.storage.data_dir || defaultDataDir

      setState((prev) => ({
        ...prev,
        backend: currentBackend as StorageBackend,
        dataDir: effectiveDataDir,
        postgresUrl: currentConfig.config.storage.postgres?.connection_string || '',
      }))
    } else {
      setState((prev) => ({
        ...prev,
        dataDir: defaultDataDir,
      }))
    }
  }, [opened, loading, currentConfig, defaultDataDir])

  const canProceedFromStorage = useMemo(() => {
    if (state.backend === 'postgres' && !isPostgresUrl(state.postgresUrl)) {
      return false
    }

    if ((!isMobilePlatform() && checkingVault) || (needsVaultPassword && !passwordValid)) {
      return false
    }

    return true
  }, [state.backend, state.postgresUrl, needsVaultPassword, passwordValid, checkingVault])

  const handleComplete = useCallback(() => {
    onComplete()

    setTimeout(() => {
      setStep(0)
      setMigrationStats(null)
      setState({
        backend: 'sqlite',
        dataDir: '',
        postgresUrl: '',
        vaultPassword: '',
        vaultPasswordConfirm: '',
        transferData: true,
      })
    }, 100)
  }, [onComplete])

  const handleSave = useCallback(async () => {
    setError(null)
    setMigrationStats(null)

    try {
      const shouldUseDefault =
        state.dataDir === defaultDataDir ||
        (isTauri() &&
          needsMigration &&
          getCurrentDataDir().includes('com.henrique.pipedash') &&
          state.dataDir === getCurrentDataDir())

      const newConfig: PipedashConfig = {
        general: currentConfig?.config.general || {
          metrics_enabled: true,
          default_refresh_interval: 30,
          status_pages_enabled: true,
          proxy_url: '',
        },
        server: currentConfig?.config.server || {
          bind_addr: '127.0.0.1:8080',
          cors_allow_all: false,
        },
        storage: {
          data_dir: shouldUseDefault ? '' : state.dataDir,
          backend: state.backend,
          ...(state.backend === 'postgres' && {
            postgres: {
              connection_string: state.postgresUrl,
            },
          }),
        },
      }

      const unlockWithEnteredPassword = async () => {
        if (!needsVaultPassword || !state.vaultPassword) {
          return true
        }

        const unlockResult = await unlockVaultMutation.mutateAsync(state.vaultPassword)

        if (!unlockResult.success) {
          setError(`Couldn't unlock the vault: ${unlockResult.message}`)
        }

        return unlockResult.success
      }

      if (!currentConfig) {
        await createInitialConfigMutation.mutateAsync({
          config: newConfig,
          ...(needsVaultPassword ? { vaultPassword: state.vaultPassword } : {}),
        })

        if (compact) {
          handleComplete()
        } else {
          goTo('success')
        }

        return
      }

      if (!(await unlockWithEnteredPassword())) {
        return
      }

      if (needsMigration && state.transferData && currentConfig) {
        const migrationOptions = {
          migrate_tokens: true,
          migrate_cache: true,
          dry_run: false,
          ...(needsVaultPassword &&
            state.vaultPassword && {
              token_password: state.vaultPassword,
            }),
        }

        const plan = await planMigrationMutation.mutateAsync({
          config: newConfig,
          options: migrationOptions,
        })

        const result = await executeMigrationMutation.mutateAsync({
          plan,
          options: migrationOptions,
        })

        if (!result.success) {
          setError(result.errors?.join(', ') || 'Transfer failed')

          return
        }

        await saveStorageConfigMutation.mutateAsync(newConfig)

        setMigrationStats(result.stats)
      } else {
        await saveStorageConfigMutation.mutateAsync(newConfig)
      }

      goTo('success')
    } catch (err) {
      setError(formatErrorMessage(err))
    }
  }, [
    currentConfig,
    state,
    needsMigration,
    compact,
    goTo,
    handleComplete,
    needsVaultPassword,
    createInitialConfigMutation,
    saveStorageConfigMutation,
    planMigrationMutation,
    executeMigrationMutation,
    unlockVaultMutation,
    defaultDataDir,
    getCurrentDataDir,
  ])

  const afterTransfer = useCallback(() => {
    setError(null)

    if (compact) {
      void handleSave()
    } else {
      goTo('confirm')
    }
  }, [compact, handleSave, goTo])

  const handleClose = useCallback(() => onClose?.(), [onClose])

  const handleContinueFromTransfer = useCallback(() => {
    if (mobileShell && !state.transferData && needsMigration) {
      const changes = []

      if (isDatabaseBackendChange) {
        changes.push('database backend')
      }
      if (isFromKeyring) {
        changes.push('credential storage (keyring → encrypted)')
      }
      if (isDataDirChange) {
        changes.push('data directory')
      }

      openConfirm({
        title: 'Skip data transfer?',
        centered: true,
        zIndex: 400,
        children: mobileShell ? (
          <Text size="sm">
            You are changing <strong>{changes.join(' and ')}</strong> without transferring data. Existing providers,
            credentials and history stay in the previous location and won't be available after switching.
          </Text>
        ) : (
          <Stack gap="sm">
            <Text size="sm">
              You are changing <strong>{changes.join(' and ')}</strong> without transferring data.
            </Text>
            <Text size="sm" fw={500}>
              This means:
            </Text>
            <Stack gap={4} ml="md">
              <Text size="sm">• Your existing providers will NOT be available in the new location</Text>
              <Text size="sm">• Your saved credentials will NOT be transferred</Text>
              <Text size="sm">• Your pipeline history will NOT be migrated</Text>
            </Stack>
            <Text size="sm" c="dimmed">
              Your old data will remain in the previous location but won't be accessible after switching. You'll need to
              reconfigure providers manually.
            </Text>
          </Stack>
        ),
        labels: {
          confirm: 'Skip data transfer',
          cancel: 'Go back',
        },
        confirmProps: { color: 'red' },
        onConfirm: afterTransfer,
      })
    } else {
      afterTransfer()
    }
  }, [
    state.transferData,
    needsMigration,
    isDatabaseBackendChange,
    isFromKeyring,
    isDataDirChange,
    mobileShell,
    afterTransfer,
  ])

  const footer = useMemo(() => {
    if (loading || isSuccessStep) {
      return null
    }

    if (currentStep === 'storage') {
      return (
        <Group justify="flex-end" gap="sm">
          {onClose && (
            <Button variant="default" size="sm" onClick={handleClose} disabled={saving} fullWidth={isMobile}>
              Cancel
            </Button>
          )}
          <Button
            size="sm"
            onClick={() => {
              setError(null)
              setStep(1)
            }}
            disabled={!canProceedFromStorage}
            fullWidth={isMobile}
          >
            Continue
          </Button>
        </Group>
      )
    }

    if (currentStep === 'transfer') {
      return (
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="sm" onClick={() => setStep(0)} fullWidth={isMobile}>
            Back
          </Button>
          <Button size="sm" onClick={handleContinueFromTransfer} fullWidth={isMobile}>
            {!state.transferData && needsMigration ? 'Switch without transferring' : 'Continue'}
          </Button>
        </Group>
      )
    }

    if (currentStep === 'confirm') {
      return (
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="sm" onClick={() => setStep(step - 1)} disabled={saving} fullWidth={isMobile}>
            Back
          </Button>
          <Button size="sm" onClick={handleSave} loading={saving} fullWidth={isMobile}>
            {needsMigration && !state.transferData
              ? 'Switch without transferring'
              : needsMigration
                ? 'Save and transfer'
                : 'Save'}
          </Button>
        </Group>
      )
    }

    return null
  }, [
    step,
    currentStep,
    loading,
    saving,
    isSuccessStep,
    state,
    needsMigration,
    canProceedFromStorage,
    onClose,
    handleClose,
    handleSave,
    handleContinueFromTransfer,
    isMobile,
  ])

  const saveLabel = needsMigration && state.transferData ? 'Save and transfer' : 'Save'

  const getShellAction = (): ModalAction | undefined => {
    if (loading || isSuccessStep) {
      return undefined
    }

    if (currentStep === 'storage') {
      if (compact && !needsMigration) {
        return currentConfig ? undefined : { label: 'Get started', onClick: handleSave, loading: saving }
      }

      return {
        label: 'Continue',
        onClick: () => {
          setError(null)
          setStep(1)
        },
        disabled: !canProceedFromStorage,
      }
    }

    if (currentStep === 'transfer') {
      return {
        label: compact ? saveLabel : 'Continue',
        onClick: handleContinueFromTransfer,
        loading: compact && saving,
      }
    }

    return { label: saveLabel, onClick: handleSave, loading: saving }
  }

  const shellAction = getShellAction()
  const canGoBack = step > 0 && !loading && !isSuccessStep
  const goBack = () => setStep((current) => Math.max(current - 1, 0))
  const stepPosition = isSuccessStep ? 'Finished' : `Step ${step + 1} of ${stepperSteps.length}`
  const regionProps = {
    stepKey: currentStep,
    position: stepPosition,
    title: isSuccessStep ? 'Setup complete' : (stepperSteps[step]?.label ?? ''),
  }

  const shellFooter =
    shellAction || canGoBack ? (
      <Stack gap={4}>
        {shellAction && (
          <Button
            size="lg"
            fullWidth
            onClick={shellAction.onClick}
            loading={shellAction.loading ?? false}
            disabled={shellAction.disabled ?? false}
          >
            {shellAction.label}
          </Button>
        )}
        {canGoBack && (
          <Button size="md" fullWidth variant="subtle" onClick={goBack} disabled={saving}>
            Back
          </Button>
        )}
      </Stack>
    ) : null

  const getCurrentStepContent = () => {
    if (loading) {
      return <LoadingState message="Loading storage configuration…" />
    }

    switch (currentStep) {
      case 'transfer':
        return <TransferStep state={state} setState={setState} error={error} />
      case 'confirm':
        return (
          <ConfirmStep
            state={state}
            showTransferStep={showTransferStep}
            vaultPasswordFromEnv={vaultPasswordFromEnv}
            isMobile={isMobile}
            error={error}
            vaultStatus={vaultStatus || null}
          />
        )
      case 'success':
        return (
          <SuccessStep
            state={state}
            migrationStats={migrationStats}
            vaultStatus={vaultStatus || null}
            vaultPasswordFromEnv={vaultPasswordFromEnv}
            onComplete={handleComplete}
          />
        )
      default:
        return (
          <StorageStep
            state={state}
            setState={setState}
            currentConfig={currentConfig || null}
            vaultStatus={vaultStatus || null}
            error={error}
            checkingVault={checkingVault}
          />
        )
    }
  }

  if (mobileShell) {
    const body = (
      <StepRegion {...regionProps}>
        {!loading && !isSuccessStep && stepperSteps.length > 1 && (
          <StepHeader step={step} labels={stepperSteps.map((s) => s.label)} />
        )}
        {getCurrentStepContent()}
      </StepRegion>
    )

    if (!onClose) {
      return (
        <Onboarding title={isSuccessStep ? undefined : 'Welcome to Pipedash'} footer={shellFooter}>
          {body}
        </Onboarding>
      )
    }

    return (
      <StandardModal
        opened={opened}
        onClose={isSuccessStep ? handleComplete : handleClose}
        title={showTransferStep ? 'Storage settings' : 'Welcome to Pipedash'}
        size="lg"
        fullHeight
        loading={saving}
        {...(shellAction ? { primaryAction: shellAction } : {})}
        {...(canGoBack ? { backAction: { label: 'Back', onClick: goBack } } : {})}
      >
        {body}
      </StandardModal>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={isSuccessStep ? handleComplete : handleClose}
      closable={isSuccessStep || Boolean(onClose)}
      title={showTransferStep ? 'Storage settings' : 'Welcome to Pipedash'}
      size="lg"
      loading={saving}
      footer={footer}
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {!loading && !isSuccessStep && (
          <Box
            px="lg"
            pt="md"
            pb="md"
            style={{
              flexShrink: 0,
              borderBottom: '1px solid var(--mantine-color-default-border)',
            }}
          >
            <Stepper active={step} size="xs" iconSize={24}>
              {stepperSteps.map((s) => (
                <Stepper.Step
                  key={s.label}
                  label={isMobile ? undefined : s.label}
                  aria-label={s.label}
                  icon={s.icon}
                  completedIcon={<IconCheck size={16} />}
                />
              ))}
            </Stepper>

            {isMobile && (
              <Text size="xs" c="dimmed" ta="center" mt="xs">
                Step {step + 1} of {stepperSteps.length}: {stepperSteps[step]?.label}
              </Text>
            )}
          </Box>
        )}

        <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <ScrollArea style={{ flex: 1 }} type="auto">
            <Box px="lg" py="lg">
              <StepRegion {...regionProps} hiddenHeading>
                {getCurrentStepContent()}
              </StepRegion>
            </Box>
          </ScrollArea>
        </Box>
      </Stack>
    </StandardModal>
  )
}
