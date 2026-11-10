import type { ReactNode, RefObject } from 'react'
import { FormErrorDisplay } from '../../../../components/common/FormErrorDisplay'
import { LiveStatus, useErrorAnnouncement } from '../../../../components/forms/LiveStatus'
import { StepRegion } from '../../../../components/forms/StepRegion'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import type { PluginMetadata } from '../../../../types/plugins'

import { CredentialsStep } from './CredentialsStep'
import { PipelinesStep } from './PipelinesStep'
import { ProviderStep } from './ProviderStep'
import { STEP_ORDER } from './types'
import type { ProviderFormModel } from './useProviderFormAction'

interface ProviderWizardFormProps {
  model: ProviderFormModel
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  editMode: boolean
  isMobile: boolean
  mobileShell: boolean
  formRef: RefObject<HTMLFormElement | null>
  formId: string
  permissionRow: ReactNode
}

const FORM_STYLE = {
  display: 'flex',
  flexDirection: 'column',
  flex: 1,
  minHeight: 0,
} as const

export function ProviderWizardForm({
  model,
  plugins,
  pluginsLoading,
  editMode,
  isMobile,
  mobileShell,
  formRef,
  formId,
  permissionRow,
}: ProviderWizardFormProps): ReactNode {
  const { state, dispatch, formAction, submitIntent, isPending, providerConfig, selectionNeedsRefresh, saveOnly } =
    model
  const missingLabels = model.check?.missing_field_labels ?? []
  const { step, selectedPlugin, fieldErrors } = state
  const items = selectedPlugin?.presentation.terminology.items ?? 'items'
  const announcement = useErrorAnnouncement(fieldErrors)
  const position = `Step ${STEP_ORDER.indexOf(step) + 1} of ${STEP_ORDER.length}`
  const titles: Record<typeof step, string> = {
    provider: 'Provider',
    credentials: 'Credentials',
    pipelines: `Select ${items}`,
  }
  const title = titles[step]

  const secondaryRow =
    mobileShell && saveOnly && step === 'credentials' ? (
      <IosGroup>
        <IosRow title={`Next: select ${items}`} chevron onClick={() => submitIntent('continue')} disabled={isPending} />
      </IosGroup>
    ) : undefined

  const failureTitle = editMode ? "Couldn't update provider" : "Couldn't add provider"
  const errorTitle = mobileShell ? "Couldn't save" : failureTitle

  const renderStep = (): ReactNode => {
    if (step === 'provider') {
      return (
        <ProviderStep
          state={state}
          dispatch={dispatch}
          plugins={plugins}
          pluginsLoading={pluginsLoading}
          editMode={editMode}
          isMobile={isMobile}
          submitting={isPending}
          formRef={formRef}
          formId={formId}
          validateKey={model.validateKey}
        />
      )
    }

    if (step === 'credentials') {
      return (
        <CredentialsStep
          state={state}
          dispatch={dispatch}
          isMobile={isMobile}
          submitting={isPending}
          formRef={formRef}
          formId={formId}
          validateKey={model.validateKey}
          missingLabels={missingLabels}
          permissionRow={permissionRow}
          secondaryRow={secondaryRow}
          scopeNote={selectionNeedsRefresh ? `Select ${items} again after changing connection scope.` : undefined}
        />
      )
    }

    return selectedPlugin ? (
      <PipelinesStep
        state={state}
        dispatch={dispatch}
        providerConfig={providerConfig}
        terms={selectedPlugin.presentation.terminology}
        isMobile={isMobile}
      />
    ) : null
  }

  return (
    <form ref={formRef} id={formId} action={formAction} noValidate style={FORM_STYLE}>
      <FormErrorDisplay
        title={errorTitle}
        globalError={model.submitError}
        onDismiss={() => dispatch({ type: 'SET_ERROR', error: null })}
      />

      <StepRegion
        stepKey={step}
        position={position}
        title={title}
        hiddenHeading={!mobileShell}
        style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
      >
        {renderStep()}
      </StepRegion>
      <LiveStatus message={announcement} />
    </form>
  )
}
