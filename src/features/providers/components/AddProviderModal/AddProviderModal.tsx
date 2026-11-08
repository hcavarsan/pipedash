import { Stack } from '@mantine/core'
import { type ReactNode, useId, useRef } from 'react'
import { LoadingState } from '../../../../components/common/LoadingState'
import { StandardModal } from '../../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../../components/ErrorBoundary/ErrorBoundary'
import { useIsMobile, useMobileShell } from '../../../../hooks/useIsMobile'
import { usePlugins } from '../../../../queries/usePluginsQueries'
import type { PluginMetadata } from '../../../../types/plugins'
import { useFocusFirstInvalid } from '../../../../utils/formFocus'
import { PermissionCheckButton } from '../PermissionCheckButton'
import { PermissionCheckModal } from '../PermissionCheckModal'

import { ProviderWizardFooter } from './ProviderWizardFooter'
import { ProviderWizardForm } from './ProviderWizardForm'
import { ProviderWizardProgress } from './ProviderWizardProgress'
import { type AddProviderModalProps, STEP_ORDER, type Step } from './types'
import { usePermissionCheck } from './usePermissionCheck'
import { useProviderFormAction } from './useProviderFormAction'

const NO_PLUGINS: PluginMetadata[] = []

type WizardSessionProps = AddProviderModalProps & { plugins: PluginMetadata[] }

function ProviderWizardSession(props: WizardSessionProps): ReactNode {
  const { opened, onClose, plugins } = props
  const editMode = props.editMode === true
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(formRef)
  const initialPlugin =
    props.editMode === true
      ? (plugins.find((plugin) => plugin.provider_type === props.existingProvider.provider_type) ?? null)
      : null
  const model = useProviderFormAction({ mode: props, initialPlugin, focusFirstInvalid })
  const { state, dispatch, dirty, isPending, providerConfig, credentialsIncomplete, selectionNeedsRefresh, saveOnly } =
    model
  const { step, selectedPlugin, providerName, selectedPipelines, selectingAll } = state
  const permission = usePermissionCheck({ selectedPlugin, providerConfig })
  const items = selectedPlugin?.presentation.terminology.items ?? 'items'
  const activeStepIndex = STEP_ORDER.indexOf(step)
  const providerSelected = !!selectedPlugin
  const providerNameValid = !!providerName.trim()

  const permissionRow =
    selectedPlugin && selectedPlugin.required_permissions.length > 0 ? (
      <PermissionCheckButton
        onClick={() => void permission.checkPermissions()}
        disabled={isPending || credentialsIncomplete}
        loading={permission.checking}
      />
    ) : undefined

  const primaryAction =
    step === 'pipelines'
      ? {
          label: editMode ? 'Update' : 'Add',
          onClick: () => model.submitIntent('save-selection'),
          loading: isPending,
          disabled: selectedPipelines.size === 0 || selectingAll,
        }
      : {
          label: step === 'credentials' && saveOnly ? 'Save' : 'Next',
          onClick: () => formRef.current?.requestSubmit(),
          loading: isPending,
          disabled: !providerSelected || !providerNameValid,
        }

  const previousStep = STEP_ORDER[activeStepIndex - 1]
  const goTo = (target: Step | undefined): void => {
    if (target && !isPending) {
      dispatch({ type: 'SET_STEP', step: target })
    }
  }
  const backAction = previousStep ? { label: 'Back', onClick: () => goTo(previousStep) } : undefined

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={editMode ? 'Edit provider' : 'Add provider'}
      loading={isPending}
      dirty={dirty}
      closable={!permission.modalOpen}
      footer={(requestClose) => (
        <ProviderWizardFooter
          step={step}
          formId={formId}
          editMode={editMode}
          saveOnly={saveOnly}
          selectionNeedsRefresh={selectionNeedsRefresh}
          isMobile={isMobile}
          providerSelected={providerSelected}
          providerNameValid={providerNameValid}
          selectedCount={selectedPipelines.size}
          selectingAll={selectingAll}
          items={items}
          permissionRow={permissionRow}
          isPending={isPending}
          onBack={() => goTo(previousStep)}
          onCancel={requestClose}
          onSaveCredentials={() => model.submitIntent('save-credentials')}
          onSaveSelection={() => model.submitIntent('save-selection')}
        />
      )}
      primaryAction={primaryAction}
      {...(backAction ? { backAction } : {})}
      contentPadding={false}
      size="lg"
      fullHeight
      disableScrollArea
    >
      <ErrorBoundary scope="feature" name="Provider wizard">
        <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <ProviderWizardProgress
            activeStepIndex={activeStepIndex}
            isMobile={isMobile}
            mobileShell={mobileShell}
            selectedPlugin={selectedPlugin}
            items={items}
            onStepSelect={(index) => goTo(STEP_ORDER[index])}
          />

          <ProviderWizardForm
            model={model}
            plugins={plugins}
            pluginsLoading={false}
            editMode={editMode}
            isMobile={isMobile}
            mobileShell={mobileShell}
            formRef={formRef}
            formId={formId}
            permissionRow={mobileShell ? permissionRow : undefined}
          />
        </Stack>
      </ErrorBoundary>

      <PermissionCheckModal
        opened={permission.modalOpen}
        onClose={permission.closeModal}
        onRetry={() => void permission.checkPermissions()}
        metadata={selectedPlugin}
        status={permission.status}
        features={permission.features}
        loading={permission.checking}
        error={permission.error}
      />
    </StandardModal>
  )
}

export function AddProviderModal(props: AddProviderModalProps): ReactNode {
  const { data, isLoading } = usePlugins()

  if (isLoading) {
    return (
      <StandardModal
        opened={props.opened}
        onClose={props.onClose}
        title={props.editMode === true ? 'Edit provider' : 'Add provider'}
        size="lg"
      >
        <LoadingState variant="section" message="Loading available providers..." />
      </StandardModal>
    )
  }

  return <ProviderWizardSession {...props} plugins={data ?? NO_PLUGINS} />
}

export type { AddProviderModalProps }
