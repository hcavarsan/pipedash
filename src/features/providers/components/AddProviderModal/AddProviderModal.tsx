import { Box, Group, Stack, Stepper, Text } from '@mantine/core'
import { IconCheck, IconKey, IconList, IconPlug } from '@tabler/icons-react'
import { type ReactNode, useId, useRef } from 'react'
import { LoadingState } from '../../../../components/common/LoadingState'
import { StandardModal } from '../../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../../components/ErrorBoundary/ErrorBoundary'
import { useIsMobile, useMobileShell } from '../../../../hooks/useIsMobile'
import { usePlugins } from '../../../../queries/usePluginsQueries'
import { iosClasses } from '../../../../theme/ios'
import type { PluginMetadata } from '../../../../types/domain'
import { useFocusFirstInvalid } from '../../../../utils/formFocus'
import { getPipelineLabels } from '../../../../utils/pipelinePresentation'
import { PermissionCheckButton } from '../PermissionCheckButton'
import { PermissionCheckModal } from '../PermissionCheckModal'

import { ProviderWizardFooter } from './ProviderWizardFooter'
import { ProviderWizardForm } from './ProviderWizardForm'
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
  const labels = getPipelineLabels(selectedPlugin?.provider_type)
  const isFlux = selectedPlugin?.provider_type === 'fluxcd'
  const activeStepIndex = STEP_ORDER.indexOf(step)
  const stepNames = ['Provider', 'Credentials', `Select ${labels.items}`]
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
          labels={labels}
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
          <Box px="lg" pt={mobileShell ? 0 : 'md'} style={{ flexShrink: 0 }}>
            {mobileShell ? (
              <Group
                className={iosClasses.stepperSteps}
                gap={6}
                justify="center"
                role="img"
                aria-label={`Step ${activeStepIndex + 1} of ${STEP_ORDER.length}`}
              >
                {STEP_ORDER.map((name, index) => (
                  <Box
                    key={name}
                    className={iosClasses.stepperStepIcon}
                    data-progress={index <= activeStepIndex || undefined}
                    aria-hidden
                  />
                ))}
              </Group>
            ) : (
              <Stepper
                active={activeStepIndex}
                size={isMobile ? 'xs' : 'sm'}
                iconSize={isMobile ? 32 : 42}
                allowNextStepsSelect={false}
                onStepClick={(index) => goTo(STEP_ORDER[index])}
              >
                <Stepper.Step
                  aria-label="Step 1: Provider"
                  label={isMobile ? undefined : 'Provider'}
                  description={isMobile ? undefined : (selectedPlugin?.name ?? 'Choose the service to monitor')}
                  icon={<IconPlug size={18} aria-hidden />}
                  completedIcon={<IconCheck size={18} aria-hidden />}
                />
                <Stepper.Step
                  aria-label="Step 2: Credentials"
                  label={isMobile ? undefined : 'Credentials'}
                  description={
                    isMobile ? undefined : isFlux ? 'Connect to your Kubernetes cluster' : 'Enter your API credentials'
                  }
                  icon={<IconKey size={18} aria-hidden />}
                  completedIcon={<IconCheck size={18} aria-hidden />}
                />
                <Stepper.Step
                  aria-label={`Step 3: Select ${labels.items}`}
                  label={isMobile ? undefined : `Select ${labels.items}`}
                  description={isMobile ? undefined : `Choose ${labels.items} to monitor`}
                  icon={<IconList size={18} aria-hidden />}
                  completedIcon={<IconCheck size={18} aria-hidden />}
                />
              </Stepper>
            )}
            {isMobile && !mobileShell && (
              <Text size="sm" fw={500} ta="center" mt="xs">
                Step {activeStepIndex + 1} of {STEP_ORDER.length}: {stepNames[activeStepIndex]}
              </Text>
            )}
          </Box>

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
