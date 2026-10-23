import { Box, Button, Group, ScrollArea, Stack, Stepper, Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { IconCheck, IconDatabase, IconListCheck, IconTransfer } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useIsMobile } from '../../../hooks/useIsMobile'
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
import { LoadingState } from '../../common/LoadingState'
import { StandardModal } from '../../common/StandardModal'

import { ConfirmStep } from './ConfirmStep'
import { StorageStep } from './StorageStep'
import { SuccessStep } from './SuccessStep'
import { TransferStep } from './TransferStep'
import type { SetupState, SetupWizardProps, StorageBackend } from './types'

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
  const isFromKeyring = useMemo(() => {
    if (!currentConfig || !isTauri()) {
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
  const showTransferStep = reconfiguring ?? Boolean(currentConfig)
  const isDatabaseBackendChange = showTransferStep && getCurrentBackend() !== state.backend
  const isDataDirChange = showTransferStep && getCurrentDataDir() !== state.dataDir

  const needsMigration = isDatabaseBackendChange || isFromKeyring || isDataDirChange

  const wizardSteps = showTransferStep
    ? (['storage', 'transfer', 'confirm', 'success'] as const)
    : (['storage', 'confirm', 'success'] as const)
  const currentStep = wizardSteps[step] ?? 'storage'
  const isSuccessStep = currentStep === 'success'

  const stepperSteps = useMemo(() => {
    const steps = [{ label: 'Storage', icon: <IconDatabase size={16} /> }]

    if (showTransferStep) {
      steps.push({ label: 'Transfer', icon: <IconTransfer size={16} /> })
    }

    steps.push({ label: 'Confirm', icon: <IconListCheck size={16} /> })

    return steps
  }, [showTransferStep])

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
    if (state.backend === 'postgres' && !state.postgresUrl.trim()) {
      return false
    }

    if (!vaultPasswordFromEnv && !passwordValid) {
      return false
    }

    return true
  }, [state.backend, state.postgresUrl, vaultPasswordFromEnv, passwordValid])

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
        if (vaultPasswordFromEnv || !state.vaultPassword) {
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
          vaultPassword: !vaultPasswordFromEnv ? state.vaultPassword : undefined,
        })

        setStep(2)

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
          ...(!vaultPasswordFromEnv &&
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

      setStep(showTransferStep ? 3 : 2)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save configuration')
    }
  }, [
    currentConfig,
    state,
    needsMigration,
    showTransferStep,
    vaultPasswordFromEnv,
    createInitialConfigMutation,
    saveStorageConfigMutation,
    planMigrationMutation,
    executeMigrationMutation,
    unlockVaultMutation,
    defaultDataDir,
    getCurrentDataDir,
  ])

  const handleComplete = () => {
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
  }

  const handleClose = useCallback(() => onClose?.(), [onClose])

  const handleContinueFromTransfer = useCallback(() => {
    if (!state.transferData && needsMigration) {
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

      modals.openConfirmModal({
        title: 'Skip data transfer?',
        centered: true,
        zIndex: 400,
        children: (
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
        onConfirm: () => {
          setError(null)
          setStep(2)
        },
      })
    } else {
      setError(null)
      setStep(2)
    }
  }, [state.transferData, needsMigration, isDatabaseBackendChange, isFromKeyring, isDataDirChange])

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
            Continue
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
            {needsMigration && state.transferData ? 'Save and transfer' : 'Save'}
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
              {getCurrentStepContent()}
            </Box>
          </ScrollArea>
        </Box>
      </Stack>
    </StandardModal>
  )
}
