import { Stack } from '@mantine/core'
import { IconKey, IconList, IconPlug } from '@tabler/icons-react'
import { type ReactNode, useId, useRef } from 'react'
import { ErrorState } from '../../../../components/common/ErrorState'
import { LoadingState } from '../../../../components/common/LoadingState'
import { StandardModal } from '../../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../../components/ErrorBoundary/ErrorBoundary'
import { WizardProgress } from '../../../../components/forms/WizardProgress'
import { useIsMobile, useMobileShell } from '../../../../hooks/useIsMobile'
import { usePlugins } from '../../../../queries/usePluginsQueries'
import type { PluginMetadata } from '../../../../types/plugins'
import { useFocusFirstInvalid } from '../../../../utils/formFocus'
import { PermissionCheckButton } from '../PermissionCheckButton'
import { PermissionCheckModal } from '../PermissionCheckModal'

import { ProviderWizardFooter } from './ProviderWizardFooter'
import { ProviderWizardForm } from './ProviderWizardForm'
import { type AddProviderModalProps, STEP_ORDER, type Step } from './types'
import { usePermissionCheck } from './usePermissionCheck'
import { useProviderFormAction } from './useProviderFormAction'

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
  const {
    state,
    dispatch,
    dirty,
    isPending,
    pendingIntent,
    providerConfig,
    credentialsIncomplete,
    selectionNeedsRefresh,
    saveOnly,
  } = model
  const { step, selectedPlugin, providerName, selectedPipelines, selectingAll } = state
  const permission = usePermissionCheck({ selectedPlugin, providerConfig })
  const items = selectedPlugin?.presentation.terminology.items ?? 'items'
  const activeStepIndex = STEP_ORDER.indexOf(step)
  const providerSelected = !!selectedPlugin
  const providerNameValid = !!providerName.trim()
  const selectionRequired = model.check?.selection_required !== false

  const permissionRow =
    selectedPlugin && selectedPlugin.required_permissions.length > 0 ? (
      <PermissionCheckButton
        onClick={() => void permission.checkPermissions()}
        disabled={isPending || credentialsIncomplete}
        loading={permission.checking}
      />
    ) : undefined

  const progressSteps = [
    {
      label: 'Provider',
      description: selectedPlugin?.name ?? 'Choose the service to monitor',
      icon: <IconPlug size={16} aria-hidden />,
    },
    {
      label: 'Credentials',
      description: selectedPlugin?.presentation.wizard.credentials_description ?? 'Enter your API credentials',
      icon: <IconKey size={16} aria-hidden />,
    },
    {
      label: `Select ${items}`,
      description: `Choose ${items} to monitor`,
      icon: <IconList size={16} aria-hidden />,
    },
  ]

  const credentialsIntent = saveOnly ? 'save-credentials' : 'continue'
  const primaryIntent = step === 'credentials' ? credentialsIntent : 'continue'
  const primaryAction =
    step === 'pipelines'
      ? {
          label: editMode ? 'Update provider' : 'Add provider',
          onClick: () => model.submitIntent('save-selection'),
          loading: pendingIntent === 'save-selection',
          disabled: (selectionRequired && selectedPipelines.size === 0) || selectingAll,
        }
      : {
          label: step === 'credentials' && saveOnly ? 'Save' : 'Continue',
          onClick: () => formRef.current?.requestSubmit(),
          loading: pendingIntent === primaryIntent,
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
          providerSelected={providerSelected}
          providerNameValid={providerNameValid}
          selectedCount={selectedPipelines.size}
          selectionRequired={selectionRequired}
          selectingAll={selectingAll}
          items={items}
          permissionRow={permissionRow}
          busy={isPending}
          pendingIntent={pendingIntent}
          onBack={() => goTo(previousStep)}
          onCancel={requestClose}
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
          <WizardProgress
            steps={progressSteps}
            active={activeStepIndex}
            onStepSelect={(index) => goTo(STEP_ORDER[index])}
          />

          <ProviderWizardForm
            model={model}
            plugins={plugins}
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
  const { data, error, isError, isFetching, refetch } = usePlugins()

  if (data === undefined) {
    return (
      <StandardModal
        opened={props.opened}
        onClose={props.onClose}
        title={props.editMode === true ? 'Edit provider' : 'Add provider'}
        size="lg"
      >
        {isError ? (
          <ErrorState
            title="Couldn't load providers"
            error={error}
            onRetry={() => void refetch()}
            retrying={isFetching}
          />
        ) : (
          <LoadingState variant="section" message="Loading available providers…" />
        )}
      </StandardModal>
    )
  }

  return <ProviderWizardSession {...props} plugins={data} />
}

export type { AddProviderModalProps }
