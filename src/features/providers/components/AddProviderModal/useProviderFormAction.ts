import { type Dispatch, startTransition, useActionState, useMemo, useReducer, useState } from 'react'
import { logger } from '../../../../lib/logger'
import { useSaveProviderDraft } from '../../../../queries/useProvidersQueries'
import { service } from '../../../../services/service'
import { isValidationError, toPipedashError } from '../../../../types/errors'
import type { PluginMetadata } from '../../../../types/plugins'
import type { ProviderDraftResult } from '../../../../types/provider_draft'
import type { ProviderConfig } from '../../../../types/providers'
import { errorMessage } from '../../../../utils/errorDisplay'
import { sameJson } from '../../../../utils/sameJson'
import { TOKEN_KEY } from '../../lib/credentials'
import { useProviderDraftCheck } from '../../queries/useProviderDraftCheck'

import { buildProviderDraft } from './draft'
import { fieldRequest, formReducer, isFormDirty } from './reducer'
import {
  type AddProviderModalProps,
  type FormAction,
  type FormState,
  initialActionState,
  initialFormState,
  type ProviderFormActionState,
  type ProviderSubmitIntent,
  type ValidateKey,
} from './types'
import { mapProviderFieldErrors, stepOfField, stepWithErrors } from './validation'

interface UseProviderFormActionParams {
  mode: AddProviderModalProps
  initialPlugin: PluginMetadata | null
  focusFirstInvalid: () => void
}

export interface ProviderFormModel {
  state: FormState
  dispatch: Dispatch<FormAction>
  dirty: boolean
  actionState: ProviderFormActionState
  formAction: (formData: FormData) => void
  submitIntent: (intent: ProviderSubmitIntent) => void
  validateKey: ValidateKey
  submitError: string | null
  isPending: boolean
  pendingIntent: ProviderSubmitIntent | null
  providerConfig: ProviderConfig
  check: ProviderDraftResult | undefined
  credentialsIncomplete: boolean
  selectionNeedsRefresh: boolean
  saveOnly: boolean
}

const SELECTION_ERROR_KEY = 'selected_items'

const INTENTS: readonly ProviderSubmitIntent[] = ['continue', 'save-credentials', 'save-selection']

interface CredentialsOutcome {
  failure: ProviderFormActionState | null
  refreshSelection: boolean
}

function initialState(mode: AddProviderModalProps, plugin: PluginMetadata | null): FormState {
  return mode.editMode === true && plugin
    ? formReducer(initialFormState, { type: 'INIT_EDIT_MODE', plugin, provider: mode.existingProvider })
    : initialFormState
}

export function useProviderFormAction({
  mode,
  initialPlugin,
  focusFirstInvalid,
}: UseProviderFormActionParams): ProviderFormModel {
  const [state, dispatch] = useReducer(formReducer, undefined, () => initialState(mode, initialPlugin))
  const { step, selectedPlugin, providerName, authMode, configValues, selectedPipelines } = state
  const editMode = mode.editMode === true
  const existingProvider = mode.editMode === true ? mode.existingProvider : undefined
  const providerId = existingProvider?.id ?? null
  const saveDraft = useSaveProviderDraft({ toastErrors: false })
  const [pendingIntent, setPendingIntent] = useState<ProviderSubmitIntent | null>(null)
  const liveDraft = useMemo(
    () => buildProviderDraft({ selectedPlugin, providerName, authMode, configValues }, providerId, null),
    [selectedPlugin, providerName, authMode, configValues, providerId]
  )
  const check = useProviderDraftCheck(liveDraft)
  const selectionNeedsRefresh = editMode && check?.selection_needs_refresh === true
  const saveOnly = editMode && !selectionNeedsRefresh
  const credentialsIncomplete = !!selectedPlugin && check?.credentials_complete !== true

  const providerConfig: ProviderConfig = {
    name: providerName,
    provider_type: selectedPlugin?.provider_type || '',
    token: configValues[TOKEN_KEY] || '',
    config: configValues,
    refresh_interval: 300,
  }

  const submittedDraft = buildProviderDraft(state, providerId, Array.from(selectedPipelines))
  const submitError = state.error && sameJson(state.error.draft, submittedDraft) ? state.error.message : null

  const fail = (error: string, fieldErrors: Record<string, string>): ProviderFormActionState => {
    dispatch({ type: 'SET_FIELD_ERRORS', errors: fieldErrors })
    dispatch({ type: 'SET_ERROR', error: { message: error, draft: submittedDraft } })

    return { error, fieldErrors }
  }

  const validateKey: ValidateKey = async (key, patch = {}) => {
    const next = { ...state, ...patch }
    const draft = buildProviderDraft(next, providerId, null)
    const requested = fieldRequest(next, key)

    if (!draft) {
      return
    }

    try {
      const { errors } = await service.validateProviderDraft(draft)
      const message = errors[key]

      dispatch(
        message
          ? { type: 'SET_FIELD_ERROR', key, error: message, requested }
          : { type: 'CLEAR_FIELD_ERROR', key, requested }
      )
    } catch (err: unknown) {
      logger.warn('AddProviderModal', 'Field validation failed', err)
    }
  }

  const validateCredentials = async (
    plugin: PluginMetadata,
    onlyProviderStep: boolean
  ): Promise<CredentialsOutcome> => {
    const draft = buildProviderDraft(state, providerId, null)
    const fields = plugin.config_schema.fields
    const result = draft ? await service.validateProviderDraft(draft) : null
    const errors = Object.fromEntries(
      Object.entries(result?.errors ?? {}).filter(
        ([key]) => key !== SELECTION_ERROR_KEY && (!onlyProviderStep || stepOfField(key, fields) === 'provider')
      )
    )

    dispatch({ type: 'SET_FIELD_ERRORS', errors })

    if (Object.keys(errors).length === 0) {
      return { failure: null, refreshSelection: result?.selection_needs_refresh === true }
    }

    dispatch({ type: 'SET_STEP', step: stepWithErrors(errors, fields) })
    focusFirstInvalid()

    return { failure: { error: null, fieldErrors: errors }, refreshSelection: false }
  }

  const save = async (plugin: PluginMetadata, includeSelection: boolean): Promise<ProviderFormActionState> => {
    const draft = buildProviderDraft(state, providerId, includeSelection ? Array.from(selectedPipelines) : null)

    try {
      dispatch({ type: 'SET_ERROR', error: null })

      if (draft) {
        await saveDraft.mutateAsync(draft)
      }

      mode.onClose()

      return initialActionState
    } catch (err: unknown) {
      const failure = toPipedashError(err)
      const fieldErrors = isValidationError(failure)
        ? mapProviderFieldErrors(failure.fields ?? {}, plugin.config_schema.fields)
        : {}
      const result = fail(
        errorMessage(err, editMode ? "Couldn't update provider" : "Couldn't add provider"),
        fieldErrors
      )

      if (Object.keys(fieldErrors).length > 0) {
        dispatch({ type: 'SET_STEP', step: stepWithErrors(fieldErrors, plugin.config_schema.fields) })
        focusFirstInvalid()
      }

      return result
    }
  }

  const execute = async (intent: ProviderSubmitIntent): Promise<ProviderFormActionState> => {
    if (!selectedPlugin) {
      return fail('Please select a provider type', {})
    }

    if (intent === 'save-selection') {
      return state.selectingAll ? initialActionState : save(selectedPlugin, true)
    }

    let outcome: CredentialsOutcome

    try {
      outcome = await validateCredentials(selectedPlugin, intent === 'continue' && step === 'provider')
    } catch (err: unknown) {
      return fail(errorMessage(err, "Couldn't validate provider"), {})
    }

    if (outcome.failure) {
      return outcome.failure
    }

    if (intent === 'save-credentials') {
      return save(selectedPlugin, false)
    }

    if (step === 'credentials' && editMode && outcome.refreshSelection) {
      dispatch({ type: 'CLEAR_PIPELINES' })
    }

    dispatch({ type: 'SET_ERROR', error: null })
    dispatch({ type: 'SET_STEP', step: step === 'provider' ? 'credentials' : 'pipelines' })

    return initialActionState
  }

  const run = async (_previous: ProviderFormActionState, formData: FormData): Promise<ProviderFormActionState> => {
    const requested = INTENTS.find((candidate) => candidate === formData.get('intent'))

    if (step === 'pipelines' && requested === undefined) {
      return initialActionState
    }

    const intent = requested ?? (step === 'credentials' && saveOnly ? 'save-credentials' : 'continue')

    setPendingIntent(intent)

    try {
      return await execute(intent)
    } finally {
      setPendingIntent(null)
    }
  }

  const [actionState, formAction, isPending] = useActionState(run, initialActionState)

  const submitIntent = (intent: ProviderSubmitIntent): void => {
    const formData = new FormData()

    formData.set('intent', intent)
    startTransition(() => formAction(formData))
  }

  return {
    state,
    dispatch,
    dirty: isFormDirty(state, existingProvider),
    actionState,
    formAction,
    submitIntent,
    validateKey,
    submitError,
    isPending,
    pendingIntent: isPending ? pendingIntent : null,
    providerConfig,
    check,
    credentialsIncomplete,
    selectionNeedsRefresh,
    saveOnly,
  }
}
