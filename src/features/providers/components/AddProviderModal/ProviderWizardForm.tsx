import type { ReactNode, RefObject } from 'react'
import { FormErrorDisplay } from '../../../../components/common/FormErrorDisplay'
import { LiveStatus, useErrorAnnouncement } from '../../../../components/forms/LiveStatus'
import { StepRegion } from '../../../../components/forms/StepRegion'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import type { PluginMetadata } from '../../../../types/domain'
import { getPipelineLabels } from '../../../../utils/pipelinePresentation'

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
  const { step, selectedPlugin, error, fieldErrors } = state
  const labels = getPipelineLabels(selectedPlugin?.provider_type)
  const announcement = useErrorAnnouncement(fieldErrors)
  const position = `Step ${STEP_ORDER.indexOf(step) + 1} of ${STEP_ORDER.length}`
  const title = step === 'provider' ? 'Provider' : step === 'credentials' ? 'Credentials' : `Select ${labels.items}`

  const secondaryRow =
    mobileShell && saveOnly && step === 'credentials' ? (
      <IosGroup>
        <IosRow
          title={`Next: select ${labels.items}`}
          chevron
          onClick={() => submitIntent('continue')}
          disabled={isPending}
        />
      </IosGroup>
    ) : undefined

  return (
    <form ref={formRef} id={formId} action={formAction} noValidate style={FORM_STYLE}>
      <FormErrorDisplay
        title={mobileShell ? "Couldn't save" : editMode ? "Couldn't update provider" : "Couldn't add provider"}
        globalError={error}
        onDismiss={() => dispatch({ type: 'SET_ERROR', error: null })}
      />

      <StepRegion
        stepKey={step}
        position={position}
        title={title}
        hiddenHeading={!mobileShell}
        style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
      >
        {step === 'provider' ? (
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
          />
        ) : step === 'credentials' ? (
          <CredentialsStep
            state={state}
            dispatch={dispatch}
            isMobile={isMobile}
            submitting={isPending}
            formRef={formRef}
            formId={formId}
            permissionRow={permissionRow}
            secondaryRow={secondaryRow}
            scopeNote={
              selectionNeedsRefresh ? `Select ${labels.items} again after changing connection scope.` : undefined
            }
          />
        ) : (
          <PipelinesStep state={state} dispatch={dispatch} providerConfig={providerConfig} isMobile={isMobile} />
        )}
      </StepRegion>
      <LiveStatus message={announcement} />
    </form>
  )
}
