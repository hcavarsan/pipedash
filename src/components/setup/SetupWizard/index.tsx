import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  Box,
  Button,
  Group,
  Loader,
  ScrollArea,
  Stack,
  Stepper,
  Text,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import {
  IconCheck,
  IconDatabase,
  IconKey,
  IconRocket,
} from '@tabler/icons-react'

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
import type { PipedashConfig } from '../../../types'
import { StandardModal } from '../../common/StandardModal'

import { ConfirmStep } from './ConfirmStep'
import { StorageStep } from './StorageStep'
import { SuccessStep } from './SuccessStep'
import { TransferStep } from './TransferStep'
import type { SetupState, SetupWizardProps, StorageBackend, TransferResult } from './types'


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
  const [transferResult, setTransferResult] = useState<TransferResult | null>(null)
  const [migrationCompleted, setMigrationCompleted] = useState(false)

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

  const stepperConfig = useMemo(() => ({
    orientation: 'horizontal' as const,
    size: 'xs' as const,
    iconSize: 24,
    showLabels: true,
    showDescriptions: false
  }), [])

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


  const isDatabaseBackendChange = getCurrentBackend() !== state.backend && currentConfig !== null
  const isDataDirChange = getCurrentDataDir() !== state.dataDir && currentConfig !== null

  const needsMigration = isDatabaseBackendChange || isFromKeyring || isDataDirChange
  const showTransferStep = currentConfig !== null


  const isSuccessStep = step === (showTransferStep ? 3 : 2)

  const stepperSteps = useMemo(() => {
    const steps = [
      { label: 'Storage', description: 'Select storage backend', icon: <IconDatabase size={18} /> },
    ]

    if (showTransferStep) {
      steps.push({ label: 'Transfer', description: 'Data transfer options', icon: <IconKey size={18} /> })
    }

    steps.push(
      { label: 'Confirm', description: 'Review configuration', icon: <IconCheck size={18} /> },
      { label: 'Success', description: 'Setup complete', icon: <IconRocket size={18} /> }
    )

    return steps
  }, [showTransferStep])

  const passwordsMatch = state.vaultPassword === state.vaultPasswordConfirm
  const passwordValid = state.vaultPassword.length >= 8 && passwordsMatch

  const needsVaultPassword = true

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
      setTransferResult(null)

      return
    }

    if (!defaultDataDir) {
      return
    }

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
  }, [opened, currentConfig, defaultDataDir])

  const canProceedFromStorage = useMemo(() => {
    if (state.backend === 'postgres' && !state.postgresUrl.trim()) {
      return false
    }

    if (needsVaultPassword && !vaultPasswordFromEnv && !passwordValid) {
      return false
    }

    return true
  }, [
    state.backend,
    state.postgresUrl,
    vaultPasswordFromEnv,
    passwordValid,
    needsVaultPassword,
  ])

  const handleSave = useCallback(async () => {
    setError(null)
    setTransferResult(null)

    try {
      const shouldUseDefault = state.dataDir === defaultDataDir ||
        (isTauri() &&
         needsMigration &&
         getCurrentDataDir().includes('com.henrique.pipedash') &&
         state.dataDir === getCurrentDataDir())

      const newConfig: PipedashConfig = {
        general: currentConfig?.config.general || {
          metrics_enabled: true,
          default_refresh_interval: 30,
        },
        server: currentConfig?.config.server || {
          bind_addr: '127.0.0.1:8080',
          cors_allow_all: true,
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

      if (!currentConfig) {
        await createInitialConfigMutation.mutateAsync({
          config: newConfig,
          vaultPassword: !vaultPasswordFromEnv ? state.vaultPassword : undefined,
        })


        setStep(2)

        return
      }

      if (!vaultPasswordFromEnv && state.vaultPassword) {
        const unlockResult = await unlockVaultMutation.mutateAsync(state.vaultPassword)

        if (!unlockResult.success) {
          setError(`Failed to set vault password: ${unlockResult.message}`)

          return
        }
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

        setTransferResult({
          success: true,
          message: `Transferred ${result.stats.providers_migrated} providers, ${result.stats.tokens_migrated} tokens, and ${result.stats.cache_entries_migrated || 0} cache entries`,
        })
        setMigrationCompleted(true)
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
      setMigrationCompleted(false)
      setTransferResult(null)
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
        changes.push('Database backend')
      }
      if (isFromKeyring) {
        changes.push('Credential storage (keyring → encrypted)')
      }
      if (isDataDirChange) {
        changes.push('Data directory')
      }

      modals.openConfirmModal({
        title: 'Skip Data Migration?',
        centered: true,
        zIndex: 400,
        children: (
          <Stack gap="sm">
            <Text size="sm">
              You are changing <strong>{changes.join(' and ')}</strong> without transferring data.
            </Text>
            <Text size="sm" fw={500}>This means:</Text>
            <Stack gap={4} ml="md">
              <Text size="sm">• Your existing providers will NOT be available in the new location</Text>
              <Text size="sm">• Your saved credentials will NOT be transferred</Text>
              <Text size="sm">• Your pipeline history will NOT be migrated</Text>
            </Stack>
            <Text size="sm" c="dimmed">
              Your old data will remain in the previous location but won't be accessible after switching.
              You'll need to reconfigure providers manually.
            </Text>
          </Stack>
        ),
        labels: {
          confirm: 'Skip Migration',
          cancel: 'Go Back'
        },
        confirmProps: { color: 'red', variant: 'light' },
        cancelProps: { variant: 'light', color: 'gray' },
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

    if (step === 0) {
      return (
        <Group justify="flex-end" gap="xs" style={{ flexWrap: isMobile ? 'wrap' : 'nowrap' }}>
          <Button
            variant="light"
            color="gray"
            onClick={handleClose}
            disabled={saving}
            fullWidth={isMobile}
          >
            Cancel
          </Button>
          <Button
            variant="light"
            color="blue"
            onClick={() => {
              setError(null)
              setStep(showTransferStep ? 1 : 1)
            }}
            disabled={!canProceedFromStorage}
            fullWidth={isMobile}
          >
            Continue
          </Button>
        </Group>
      )
    }

    if (showTransferStep && step === 1) {
      return (
        <Group justify="space-between" gap="xs" style={{ flexWrap: isMobile ? 'wrap' : 'nowrap' }}>
          <Button
            variant="light"
            color="gray"
            onClick={() => setStep(0)}
            fullWidth={isMobile}
          >
            Back
          </Button>
          <Button
            variant="light"
            color="blue"
            onClick={handleContinueFromTransfer}
            fullWidth={isMobile}
          >
            Continue
          </Button>
        </Group>
      )
    }

    const confirmStep = showTransferStep ? 2 : 1

    if (step === confirmStep) {
      return (
        <Group justify="space-between" gap="xs" style={{ flexWrap: isMobile ? 'wrap' : 'nowrap' }}>
          <Button
            variant="light"
            color="gray"
            onClick={() => setStep(showTransferStep ? 1 : 0)}
            disabled={saving}
            fullWidth={isMobile}
          >
            Back
          </Button>
          <Button
            variant="light"
            color="blue"
            onClick={handleSave}
            loading={saving}
            fullWidth={isMobile}
          >
            {needsMigration && state.transferData ? 'Save & Transfer' : 'Save'}
          </Button>
        </Group>
      )
    }

    return null
  }, [step, loading, saving, showTransferStep, isSuccessStep, state, needsMigration, canProceedFromStorage, handleClose, handleSave, handleContinueFromTransfer, isMobile])

  const getCurrentStepContent = () => {
    if (loading) {
      return (
        <Stack align="center" py="xl" gap="xs">
          <Loader size="md" />
          <Text size="sm" c="dimmed">Loading config...</Text>
        </Stack>
      )
    }

    if (showTransferStep) {
      switch (step) {
        case 0:
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
        case 1:
          return (
            <TransferStep
              state={state}
              setState={setState}
              error={error}
            />
          )
        case 2:
          return (
            <ConfirmStep
              state={state}
              isDatabaseBackendChange={isDatabaseBackendChange}
              isFromKeyring={isFromKeyring}
              isDataDirChange={isDataDirChange}
              showTransferStep={showTransferStep}
              vaultPasswordFromEnv={vaultPasswordFromEnv}
              needsVaultPassword={needsVaultPassword}
              isMobile={isMobile}
              error={error}
              getCurrentBackend={getCurrentBackend}
              getCurrentDataDir={getCurrentDataDir}
              vaultStatus={vaultStatus || null}
            />
          )
        case 3:
          return (
            <SuccessStep
              state={state}
              migrationCompleted={migrationCompleted}
              transferResult={transferResult}
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
    } else {
      switch (step) {
        case 0:
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
        case 1:
          return (
            <ConfirmStep
              state={state}
              isDatabaseBackendChange={isDatabaseBackendChange}
              isFromKeyring={isFromKeyring}
              isDataDirChange={isDataDirChange}
              showTransferStep={showTransferStep}
              vaultPasswordFromEnv={vaultPasswordFromEnv}
              needsVaultPassword={needsVaultPassword}
              isMobile={isMobile}
              error={error}
              getCurrentBackend={getCurrentBackend}
              getCurrentDataDir={getCurrentDataDir}
              vaultStatus={vaultStatus || null}
            />
          )
        case 2:
          return (
            <SuccessStep
              state={state}
              migrationCompleted={migrationCompleted}
              transferResult={transferResult}
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
  }

  const getTitle = () => {
    if (isSuccessStep) {
      return 'Setup Complete'
    }

    return currentConfig ? 'Storage Settings' : 'Welcome to Pipedash'
  }

  return (
    <StandardModal
      opened={opened}
      onClose={handleClose}
      title={getTitle()}
      loading={saving}
      footer={footer}
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {!loading && !isSuccessStep && (
          <Box
            px="md"
            pt="xl"
            pb="lg"
            style={{
              flexShrink: 0,
              borderBottom: '1px solid var(--mantine-color-default-border)',
              backgroundColor: 'var(--mantine-color-dark-8)',
            }}
          >
            <Stepper
              active={step}
              size={stepperConfig.size}
              iconSize={stepperConfig.iconSize}
              orientation={stepperConfig.orientation}
              styles={{
                root: {
                  transition: 'all 0.3s ease'
                },
                stepLabel: {
                  fontSize: '0.875rem',
                  display: isMobile ? 'none' : 'block'
                },
                step: {
                  padding: isMobile ? 4 : 8
                }
              }}
            >
              {stepperSteps.map((s, i) => (
                <Stepper.Step
                  key={i}
                  label={s.label}
                  icon={s.icon}
                  completedIcon={<IconCheck size={16} />}
                />
              ))}
            </Stepper>

            {isMobile && (
              <Text size="xs" c="dimmed" ta="center" mt="xs">
                Step {step + 1} of {stepperSteps.length}
              </Text>
            )}
          </Box>
        )}

        <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <ScrollArea style={{ flex: 1 }} type="auto">
            <Box
              px={isMobile ? 'xs' : isMobile ? 'sm' : 'md'}
              pt={isSuccessStep ? (isMobile ? 'xs' : 'md') : (isMobile ? 'md' : 'xl')}
              pb={isMobile ? 'xs' : 'md'}
              style={{
                maxWidth: '100%',
                margin: '0 auto',
                animation: 'fadeIn 0.3s ease-in',
              }}
            >
              <style>
                {`
                  @keyframes fadeIn {
                    from {
                      opacity: 0;
                      transform: translateY(10px);
                    }
                    to {
                      opacity: 1;
                      transform: translateY(0);
                    }
                  }
                `}
              </style>
              {getCurrentStepContent()}
            </Box>
          </ScrollArea>
        </Box>
      </Stack>
    </StandardModal>
  )
}
