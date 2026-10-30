import { type Dispatch, startTransition, useActionState, useReducer } from 'react'
import type { PluginMetadata, ProviderConfig } from '../../../../types/domain'
import { isValidationError, toPipedashError } from '../../../../types/errors'
import { errorMessage } from '../../../../utils/errorDisplay'
import { getPipelineLabels } from '../../../../utils/pipelinePresentation'
import { SIGN_IN_KEY, TOKEN_KEY } from '../../lib/credentials'
import { useSignInStatus } from '../../queries/useSignInQueries'

import { formReducer, isFormDirty } from './reducer'
import {
  type AddProviderModalProps,
  type FormAction,
  type FormState,
  initialActionState,
  initialFormState,
  type ProviderFormActionState,
  type ProviderSubmitIntent,
} from './types'
import {
  activeConfigFields,
  activeConfigValues,
  mapProviderFieldErrors,
  validateProviderCredentials,
  validateSignIn,
} from './validation'

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
  isPending: boolean
  providerConfig: ProviderConfig
  credentialsIncomplete: boolean
  selectionNeedsRefresh: boolean
  saveOnly: boolean
}

const INTENTS: readonly ProviderSubmitIntent[] = ['continue', 'save-credentials', 'save-selection']

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
  const { data: signInStatus } = useSignInStatus(authMode === 'sign_in' ? (configValues[SIGN_IN_KEY] ?? null) : null)
  const selectionNeedsRefresh = editMode && !!existingProvider?.config.selected_items && !configValues.selected_items
  const saveOnly = editMode && !selectionNeedsRefresh
  const fields = selectedPlugin?.config_schema.fields ?? []
  const authModes = selectedPlugin?.config_schema.auth_modes

  const credentialsIncomplete =
    !!selectedPlugin &&
    (activeConfigFields(fields, authMode, authModes).some(
      (field) => field.required && !configValues[field.key]?.trim()
    ) ||
      validateSignIn(authMode, configValues, signInStatus) !== null)

  const providerConfig: ProviderConfig = {
    name: providerName,
    provider_type: selectedPlugin?.provider_type || '',
    token: configValues[TOKEN_KEY] || '',
    config: activeConfigValues(configValues, authMode, authModes),
    refresh_interval: 300,
  }

  const fail = (error: string | null, fieldErrors: Record<string, string>): ProviderFormActionState => {
    dispatch({ type: 'SET_FIELD_ERRORS', errors: fieldErrors })
    dispatch({ type: 'SET_ERROR', error })

    return { error, fieldErrors }
  }

  const validateCredentials = (plugin: PluginMetadata): ProviderFormActionState | null => {
    const errors = validateProviderCredentials({
      providerName,
      fields: plugin.config_schema.fields,
      authMode,
      authModes: plugin.config_schema.auth_modes,
      configValues,
      signInStatus,
    })

    dispatch({ type: 'SET_FIELD_ERRORS', errors })

    if (Object.keys(errors).length === 0) {
      return null
    }

    focusFirstInvalid()

    return { error: null, fieldErrors: errors }
  }

  const save = async (plugin: PluginMetadata, includeSelection: boolean): Promise<ProviderFormActionState> => {
    const { [TOKEN_KEY]: token = '', ...config } = activeConfigValues(
      configValues,
      authMode,
      plugin.config_schema.auth_modes
    )

    if (includeSelection) {
      config.selected_items = Array.from(selectedPipelines).join(',')
    }

    const savedConfig: ProviderConfig = {
      name: providerName,
      provider_type: plugin.provider_type,
      token,
      config,
      refresh_interval: existingProvider?.refresh_interval ?? 30,
    }

    try {
      dispatch({ type: 'SET_ERROR', error: null })

      if (mode.editMode === true) {
        await mode.onUpdate(mode.existingProvider.id, savedConfig)
      } else {
        await mode.onAdd(savedConfig)
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
        dispatch({ type: 'SET_STEP', step: 'credentials' })
        focusFirstInvalid()
      }

      return result
    }
  }

  const run = async (_previous: ProviderFormActionState, formData: FormData): Promise<ProviderFormActionState> => {
    const requested = INTENTS.find((candidate) => candidate === formData.get('intent'))

    if (step === 'pipelines' && requested === undefined) {
      return initialActionState
    }

    const intent = requested ?? (saveOnly ? 'save-credentials' : 'continue')

    if (!selectedPlugin) {
      return fail('Please select a provider type', {})
    }

    if (intent === 'save-selection') {
      if (selectedPipelines.size === 0) {
        return fail(`Please select at least one ${getPipelineLabels(selectedPlugin.provider_type).item}`, {})
      }

      return save(selectedPlugin, true)
    }

    const invalid = validateCredentials(selectedPlugin)

    if (invalid) {
      return invalid
    }

    if (intent === 'save-credentials') {
      return save(selectedPlugin, false)
    }

    dispatch({ type: 'SET_ERROR', error: null })
    dispatch({ type: 'SET_STEP', step: 'pipelines' })

    return initialActionState
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
    isPending,
    providerConfig,
    credentialsIncomplete,
    selectionNeedsRefresh,
    saveOnly,
  }
}
