import { useActionState } from 'react'

import { useApplySetupDraft } from '../../../../queries/useStorageQueries'
import { service } from '../../../../services/service'
import { formatErrorMessage } from '../../../../types/errors'
import type { SetupDraft } from '../../../../types/setup'
import type { MigrationResult } from '../../../../types/storage'

import { toFieldErrors, toSetupDraft } from './setupDraft'
import type { SetupFieldErrors, SetupState, WizardStep } from './types'

export interface SetupActionState {
  error: string | null
  fieldErrors: SetupFieldErrors
  migrationStats: MigrationResult['stats'] | null
  saved: boolean
  errorStep: WizardStep | null
  submitted: SetupDraft | null
}

export const INITIAL_SETUP_ACTION_STATE: SetupActionState = {
  error: null,
  fieldErrors: {},
  migrationStats: null,
  saved: false,
  errorStep: null,
  submitted: null,
}

export function setupFailure(
  step: WizardStep,
  error: string,
  submitted: SetupDraft,
  fieldErrors: SetupFieldErrors = {}
): SetupActionState {
  return { ...INITIAL_SETUP_ACTION_STATE, error, fieldErrors, errorStep: step, submitted }
}

interface SetupSubmissionInput {
  draft: SetupState
  firstRun: boolean
  step: WizardStep
  submits: boolean
  compact: boolean
  onComplete: () => void
}

export function useSetupSubmission({
  draft,
  firstRun,
  step,
  submits,
  compact,
  onComplete,
}: SetupSubmissionInput): [SetupActionState, () => void, boolean] {
  const apply = useApplySetupDraft({ toastErrors: false })

  const submit = async (previous: SetupActionState): Promise<SetupActionState> => {
    if (!submits) {
      return previous
    }

    const setupDraft = toSetupDraft(draft)

    try {
      const { errors } = await service.prepareSetupDraft(setupDraft, firstRun)
      const [firstError] = Object.values(errors)

      if (firstError) {
        return setupFailure(step, firstError, setupDraft, toFieldErrors(errors))
      }

      const applied = await apply.mutateAsync({ draft: setupDraft, firstRun })

      if (compact && firstRun) {
        onComplete()

        return INITIAL_SETUP_ACTION_STATE
      }

      return { ...INITIAL_SETUP_ACTION_STATE, saved: applied.saved, migrationStats: applied.migration_stats }
    } catch (error) {
      return setupFailure(step, formatErrorMessage(error), setupDraft)
    }
  }

  return useActionState(submit, INITIAL_SETUP_ACTION_STATE)
}
