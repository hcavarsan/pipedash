import { useActionState } from 'react'

import {
  useCreateInitialConfig,
  useExecuteMigration,
  usePlanStorageMigration,
  useSaveStorageConfig,
} from '../../../../queries/useStorageQueries'
import { useUnlockVault } from '../../../../queries/useVaultQueries'
import { isTauri } from '../../../../services/isTauri'
import type { MigrationResult, StorageConfigResponse } from '../../../../types/domain'
import { formatErrorMessage } from '../../../../types/errors'

import { buildStorageConfig, type SetupFieldErrors, validateSetupDraft } from './setupDraft'
import type { SetupState, WizardStep } from './types'

export interface SetupActionState {
  error: string | null
  fieldErrors: SetupFieldErrors
  migrationStats: MigrationResult['stats'] | null
  saved: boolean
  errorStep: WizardStep | null
}

export const INITIAL_SETUP_ACTION_STATE: SetupActionState = {
  error: null,
  fieldErrors: {},
  migrationStats: null,
  saved: false,
  errorStep: null,
}

export function setupFailure(step: WizardStep, error: string, fieldErrors: SetupFieldErrors = {}): SetupActionState {
  return { ...INITIAL_SETUP_ACTION_STATE, error, fieldErrors, errorStep: step }
}

interface SetupSubmissionInput {
  draft: SetupState
  currentConfig: StorageConfigResponse | null
  defaultDataDir: string
  step: WizardStep
  submits: boolean
  needsVaultPassword: boolean
  needsMigration: boolean
  compact: boolean
  onComplete: () => void
}

export function useSetupSubmission({
  draft,
  currentConfig,
  defaultDataDir,
  step,
  submits,
  needsVaultPassword,
  needsMigration,
  compact,
  onComplete,
}: SetupSubmissionInput): [SetupActionState, () => void, boolean] {
  const createInitialConfig = useCreateInitialConfig()
  const saveStorageConfig = useSaveStorageConfig()
  const planMigration = usePlanStorageMigration()
  const executeMigration = useExecuteMigration()
  const unlockVault = useUnlockVault()

  const currentDataDir = currentConfig?.config.storage.data_dir || defaultDataDir
  const shouldUseDefaultDir =
    draft.dataDir === defaultDataDir ||
    (isTauri() &&
      needsMigration &&
      currentDataDir.includes('com.henrique.pipedash') &&
      draft.dataDir === currentDataDir)

  const save = async (): Promise<SetupActionState> => {
    const config = buildStorageConfig(draft, currentConfig, shouldUseDefaultDir)

    if (!currentConfig) {
      await createInitialConfig.mutateAsync({
        config,
        ...(needsVaultPassword ? { vaultPassword: draft.vaultPassword } : {}),
      })

      if (compact) {
        onComplete()

        return INITIAL_SETUP_ACTION_STATE
      }

      return { ...INITIAL_SETUP_ACTION_STATE, saved: true }
    }

    if (needsVaultPassword && draft.vaultPassword) {
      const unlocked = await unlockVault.mutateAsync(draft.vaultPassword)

      if (!unlocked.success) {
        return setupFailure(step, `Couldn't unlock the vault: ${unlocked.message}`)
      }
    }

    if (!needsMigration || !draft.transferData) {
      await saveStorageConfig.mutateAsync(config)

      return { ...INITIAL_SETUP_ACTION_STATE, saved: true }
    }

    const options = {
      migrate_tokens: true,
      migrate_cache: true,
      dry_run: false,
      ...(needsVaultPassword && draft.vaultPassword && { token_password: draft.vaultPassword }),
    }
    const plan = await planMigration.mutateAsync({ config, options })
    const result = await executeMigration.mutateAsync({ plan, options })

    if (!result.success) {
      return setupFailure(step, result.errors?.join(', ') || 'Transfer failed')
    }

    await saveStorageConfig.mutateAsync(config)

    return { ...INITIAL_SETUP_ACTION_STATE, saved: true, migrationStats: result.stats }
  }

  const submit = async (previous: SetupActionState): Promise<SetupActionState> => {
    if (!submits) {
      return previous
    }

    const fieldErrors = validateSetupDraft(draft, needsVaultPassword)
    const [firstError] = Object.values(fieldErrors)

    if (firstError) {
      return setupFailure(step, firstError, fieldErrors)
    }

    try {
      return await save()
    } catch (error) {
      return setupFailure(step, formatErrorMessage(error))
    }
  }

  return useActionState(submit, INITIAL_SETUP_ACTION_STATE)
}
