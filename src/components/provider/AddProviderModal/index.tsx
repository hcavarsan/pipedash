import { Box, Button, Group, Stack, Stepper, Text } from '@mantine/core'
import { IconCheck, IconKey, IconList } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useReducer, useRef } from 'react'

import { usePlugins } from '../../../contexts/PluginContext'
import { useDiscardGuard } from '../../../hooks/useDiscardGuard'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import type { ProviderConfig } from '../../../types'
import { isAuthError, isNetworkError, isValidationError, toPipedashError } from '../../../types/errors'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { useFocusFirstInvalid } from '../../../utils/formFocus'
import { getPipelineLabels } from '../../../utils/pipelinePresentation'
import { FormErrorDisplay } from '../../common/FormErrorDisplay'
import { LoadingState } from '../../common/LoadingState'
import { StandardModal } from '../../common/StandardModal'
import { StepRegion } from '../../forms/StepRegion'
import { IosGroup, IosRow } from '../../ios/IosList'
import { PermissionCheckButton } from '../PermissionCheckButton'
import { PermissionCheckModal } from '../PermissionCheckModal'

import { CredentialsStep } from './CredentialsStep'
import { PipelinesStep } from './PipelinesStep'
import { formReducer, isFormDirty } from './reducer'
import { type AddProviderModalProps, initialFormState } from './types'
import { usePermissionCheck } from './usePermissionCheck'
import { validateConfigField, validateProviderName } from './validation'

export function AddProviderModal({
  opened,
  onClose,
  onAdd,
  onUpdate,
  editMode = false,
  existingProvider,
}: AddProviderModalProps) {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const { plugins, loading: pluginsLoading } = usePlugins()

  const [state, dispatch] = useReducer(formReducer, initialFormState)
  const { step, selectedPlugin, providerName, configValues, selectedPipelines, submitting, error } = state
  const labels = getPipelineLabels(selectedPlugin?.provider_type)
  const isFlux = selectedPlugin?.provider_type === 'fluxcd'
  const selectionNeedsRefresh = editMode && !!existingProvider?.config.selected_items && !configValues.selected_items

  const providerConfig = useMemo<ProviderConfig>(
    () => ({
      name: providerName,
      provider_type: selectedPlugin?.provider_type || '',
      token: configValues.token || '',
      config: configValues,
      refresh_interval: 300,
    }),
    [providerName, selectedPlugin?.provider_type, configValues]
  )

  const permission = usePermissionCheck({
    selectedPlugin,
    providerConfig,
  })

  const { reset: resetPermission } = permission
  const credentialsRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(credentialsRef)
  const dirty = isFormDirty(state, editMode ? existingProvider : undefined)
  const requestClose = useDiscardGuard(dirty, onClose)

  useEffect(() => {
    if (!opened) {
      dispatch({ type: 'RESET' })
      resetPermission()
    } else if (opened && editMode && existingProvider) {
      const plugin = plugins.find((p) => p.provider_type === existingProvider.provider_type)

      if (plugin) {
        dispatch({ type: 'INIT_EDIT_MODE', plugin, provider: existingProvider })
      }
    }
  }, [opened, editMode, existingProvider, plugins, resetPermission])

  const validateCredentials = useCallback((): boolean => {
    const errors: Record<string, string> = {}
    const nameError = validateProviderName(providerName)

    if (nameError) {
      errors.providerName = nameError
    }

    if (!selectedPlugin) {
      dispatch({ type: 'SET_ERROR', error: 'Please select a provider type' })

      return false
    }

    for (const field of selectedPlugin.config_schema.fields) {
      const fieldError = validateConfigField(field, configValues[field.key] || '')

      if (fieldError) {
        errors[field.key] = fieldError
      }
    }

    dispatch({ type: 'SET_FIELD_ERRORS', errors })

    if (Object.keys(errors).length > 0) {
      focusFirstInvalid()
    }

    return Object.keys(errors).length === 0
  }, [providerName, selectedPlugin, configValues, focusFirstInvalid])

  const handleNext = useCallback(() => {
    if (!validateCredentials() || !selectedPlugin) {
      return
    }

    dispatch({ type: 'SET_ERROR', error: null })
    dispatch({ type: 'SET_STEP', step: 'pipelines' })
  }, [validateCredentials, selectedPlugin])

  const handleBack = useCallback(() => {
    dispatch({ type: 'SET_STEP', step: 'credentials' })
  }, [])

  const handleSubmit = useCallback(async () => {
    if (!selectedPlugin || selectedPipelines.size === 0) {
      dispatch({ type: 'SET_ERROR', error: `Please select at least one ${labels.item}` })

      return
    }

    let success = false

    try {
      dispatch({ type: 'SET_SUBMITTING', submitting: true })
      dispatch({ type: 'SET_ERROR', error: null })

      const finalConfig = { ...configValues }
      const token = finalConfig.token || ''

      delete finalConfig.token
      finalConfig.selected_items = Array.from(selectedPipelines).join(',')

      const providerConfig: ProviderConfig = {
        name: providerName,
        provider_type: selectedPlugin.provider_type,
        token,
        config: finalConfig,
        refresh_interval: existingProvider?.refresh_interval ?? 30,
      }

      if (editMode && existingProvider && onUpdate) {
        await onUpdate(existingProvider.id, providerConfig)
      } else if (onAdd) {
        await onAdd(providerConfig)
      }

      success = true
    } catch (err: unknown) {
      const error = toPipedashError(err)

      if (isValidationError(error)) {
        dispatch({ type: 'SET_ERROR', error: 'Validation failed. Please check your input.' })
      } else if (isAuthError(error)) {
        dispatch({ type: 'SET_ERROR', error: 'Authentication failed. Please check your credentials.' })
      } else if (isNetworkError(error)) {
        dispatch({ type: 'SET_ERROR', error: 'Network error. Please check your connection.' })
      }

      displayErrorNotification(err, editMode ? "Couldn't update provider" : "Couldn't add provider")
    } finally {
      dispatch({ type: 'SET_SUBMITTING', submitting: false })
      if (success) {
        onClose()
      }
    }
  }, [
    selectedPlugin,
    selectedPipelines,
    configValues,
    providerName,
    editMode,
    existingProvider,
    onUpdate,
    onAdd,
    onClose,
    labels.item,
  ])

  const handleSaveCredentialsOnly = useCallback(async () => {
    if (!validateCredentials() || !selectedPlugin) {
      return
    }

    try {
      dispatch({ type: 'SET_SUBMITTING', submitting: true })
      dispatch({ type: 'SET_ERROR', error: null })

      const finalConfig = { ...configValues }
      const token = finalConfig.token || ''

      delete finalConfig.token

      const providerConfig: ProviderConfig = {
        name: providerName,
        provider_type: selectedPlugin.provider_type,
        token,
        config: finalConfig,
        refresh_interval: existingProvider?.refresh_interval ?? 30,
      }

      if (existingProvider && onUpdate) {
        await onUpdate(existingProvider.id, providerConfig)
      }

      onClose()
    } catch (err: unknown) {
      displayErrorNotification(err, "Couldn't update provider")
    } finally {
      dispatch({ type: 'SET_SUBMITTING', submitting: false })
    }
  }, [validateCredentials, selectedPlugin, configValues, providerName, existingProvider, onUpdate, onClose])

  const footer = useMemo(() => {
    if (step === 'credentials') {
      return (
        <Group justify="space-between" gap="xs">
          <Group gap="xs">
            {selectedPlugin && selectedPlugin.required_permissions.length > 0 && (
              <PermissionCheckButton
                onClick={permission.checkPermissions}
                disabled={
                  submitting ||
                  selectedPlugin.config_schema.fields.some(
                    (field) => field.required && !configValues[field.key]?.trim()
                  )
                }
                loading={permission.checking}
              />
            )}
            {selectionNeedsRefresh && (
              <Text size="xs" c="dimmed">
                Select {labels.items} again after changing connection scope.
              </Text>
            )}
          </Group>

          <Group gap="xs">
            <Button variant="default" size="sm" onClick={requestClose} disabled={submitting}>
              Cancel
            </Button>
            {editMode && (
              <Button
                size="sm"
                onClick={handleSaveCredentialsOnly}
                loading={submitting}
                disabled={!selectedPlugin || !providerName.trim() || selectionNeedsRefresh}
              >
                Save
              </Button>
            )}
            <Button
              variant={editMode ? 'default' : 'filled'}
              size="sm"
              onClick={handleNext}
              disabled={!selectedPlugin || !providerName.trim()}
            >
              {isMobile ? 'Next' : `Next: select ${labels.items}`}
            </Button>
          </Group>
        </Group>
      )
    }

    return (
      <Group justify="space-between">
        <Group gap="sm">
          <Button variant="default" size="sm" onClick={handleBack} disabled={submitting}>
            Back
          </Button>
          {selectedPipelines.size > 0 && (
            <Text size="sm" c="dimmed">
              {selectedPipelines.size} selected
            </Text>
          )}
        </Group>
        <Group gap="xs">
          <Button variant="default" size="sm" onClick={requestClose} disabled={submitting}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleSubmit} loading={submitting} disabled={selectedPipelines.size === 0}>
            {editMode ? (isMobile ? 'Update' : 'Update provider') : isMobile ? 'Add' : 'Add provider'}
          </Button>
        </Group>
      </Group>
    )
  }, [
    step,
    selectedPlugin,
    configValues,
    submitting,
    permission,
    editMode,
    providerName,
    isMobile,
    selectedPipelines.size,
    handleNext,
    handleBack,
    handleSubmit,
    handleSaveCredentialsOnly,
    requestClose,
    labels.items,
    selectionNeedsRefresh,
  ])

  const permissionRow =
    selectedPlugin && selectedPlugin.required_permissions.length > 0 ? (
      <PermissionCheckButton
        onClick={permission.checkPermissions}
        disabled={
          submitting ||
          selectedPlugin.config_schema.fields.some((field) => field.required && !configValues[field.key]?.trim())
        }
        loading={permission.checking}
      />
    ) : undefined

  const saveOnly = editMode && !selectionNeedsRefresh

  const submitCredentials = () => {
    if (submitting) {
      return
    }

    if (saveOnly) {
      void handleSaveCredentialsOnly()
    } else {
      handleNext()
    }
  }

  const primaryAction =
    step === 'credentials'
      ? {
          label: saveOnly ? 'Save' : 'Next',
          onClick: submitCredentials,
          loading: submitting,
          disabled: !selectedPlugin || !providerName.trim(),
        }
      : {
          label: editMode ? 'Update' : 'Add',
          onClick: handleSubmit,
          loading: submitting,
          disabled: selectedPipelines.size === 0,
        }

  const backAction = step === 'pipelines' ? { label: 'Back', onClick: () => !submitting && handleBack() } : undefined

  const secondaryRow =
    mobileShell && saveOnly && step === 'credentials' ? (
      <IosGroup>
        <IosRow title={`Next: select ${labels.items}`} chevron onClick={handleNext} disabled={submitting} />
      </IosGroup>
    ) : undefined

  const activeStepIndex = step === 'credentials' ? 0 : 1

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={editMode ? 'Edit provider' : 'Add provider'}
      loading={submitting}
      dirty={dirty}
      closable={!permission.modalOpen}
      footer={footer}
      primaryAction={primaryAction}
      backAction={backAction}
      contentPadding={false}
      size="lg"
      fullHeight
      disableScrollArea
    >
      <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <Box px="lg" pt={mobileShell ? 0 : 'md'} style={{ flexShrink: 0 }}>
          <Stepper
            active={activeStepIndex}
            size={isMobile ? 'xs' : 'sm'}
            iconSize={mobileShell ? 8 : isMobile ? 32 : 42}
          >
            <Stepper.Step
              aria-label="Step 1: Credentials"
              label={isMobile ? undefined : 'Credentials'}
              description={
                isMobile ? undefined : isFlux ? 'Connect to your Kubernetes cluster' : 'Enter your API credentials'
              }
              icon={<IconKey size={18} />}
              completedIcon={<IconCheck size={18} />}
            />
            <Stepper.Step
              aria-label={`Step 2: Select ${labels.items}`}
              label={isMobile ? undefined : `Select ${labels.items}`}
              description={isMobile ? undefined : `Choose ${labels.items} to monitor`}
              icon={<IconList size={18} />}
              completedIcon={<IconCheck size={18} />}
            />
          </Stepper>
          {isMobile && !mobileShell && (
            <Text size="sm" fw={500} ta="center" mt="xs">
              Step {activeStepIndex + 1} of 2: {activeStepIndex === 0 ? 'Credentials' : `Select ${labels.items}`}
            </Text>
          )}
        </Box>

        {pluginsLoading ? (
          <LoadingState variant="section" message="Loading available providers..." />
        ) : (
          <>
            <FormErrorDisplay globalError={error} onDismiss={() => dispatch({ type: 'SET_ERROR', error: null })} />

            <StepRegion
              stepKey={step}
              position={`Step ${activeStepIndex + 1} of 2`}
              title={step === 'credentials' ? 'Credentials' : `Select ${labels.items}`}
              hiddenHeading={!mobileShell}
              style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
            >
              {step === 'credentials' && (
                <CredentialsStep
                  state={state}
                  dispatch={dispatch}
                  plugins={plugins}
                  pluginsLoading={pluginsLoading}
                  editMode={editMode}
                  isMobile={isMobile}
                  formRef={credentialsRef}
                  onSubmit={submitCredentials}
                  permissionRow={permissionRow}
                  secondaryRow={secondaryRow}
                  scopeNote={
                    selectionNeedsRefresh ? `Select ${labels.items} again after changing connection scope.` : undefined
                  }
                />
              )}

              {step === 'pipelines' && (
                <PipelinesStep state={state} dispatch={dispatch} providerConfig={providerConfig} isMobile={isMobile} />
              )}
            </StepRegion>
          </>
        )}
      </Stack>

      <PermissionCheckModal
        opened={permission.modalOpen}
        onClose={permission.closeModal}
        onRetry={permission.checkPermissions}
        metadata={selectedPlugin}
        status={permission.status}
        features={permission.features}
        loading={permission.checking}
        error={permission.error}
      />
    </StandardModal>
  )
}

export type { AddProviderModalProps }
