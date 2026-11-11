import { Box, Stack } from '@mantine/core'
import type { ReactNode, RefObject } from 'react'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import { StandardModal } from '../../../../components/common/StandardModal'
import type { ModalAction, StandardModalProps } from '../../../../components/common/standardModalTypes'
import { ErrorBoundary } from '../../../../components/ErrorBoundary/ErrorBoundary'
import { StepRegion } from '../../../../components/forms/StepRegion'
import { WizardProgress } from '../../../../components/forms/WizardProgress'
import { useMobileShell } from '../../../../hooks/useIsMobile'

import { Onboarding, StepHeader } from './Onboarding'

export interface WizardStepper {
  steps: { label: string; icon: ReactNode }[]
  active: number
}

export interface WizardRegion {
  stepKey: string
  position: string
  title: string
}

export interface WizardForm {
  id: string
  ref: RefObject<HTMLFormElement | null>
  action: (formData: FormData) => void
}

interface SetupWizardLayoutProps {
  opened: boolean
  settings: boolean
  closable: boolean
  onClose: () => void
  title: string
  onboardingTitle: string | undefined
  busy: boolean
  stepper: WizardStepper | null
  region: WizardRegion | null
  form: WizardForm | null
  footer: StandardModalProps['footer']
  shellFooter: ReactNode
  dirty: boolean
  primaryAction: ModalAction | undefined
  onBack: (() => void) | undefined
  children: ReactNode
}

export function SetupWizardLayout({
  opened,
  settings,
  closable,
  onClose,
  title,
  onboardingTitle,
  busy,
  stepper,
  region,
  form,
  footer,
  dirty,
  shellFooter,
  primaryAction,
  onBack,
  children,
}: SetupWizardLayoutProps): ReactNode {
  const mobileShell = useMobileShell()

  const regionBody = region ? (
    <StepRegion {...region} hiddenHeading={!mobileShell}>
      {mobileShell && stepper && stepper.steps.length > 1 && (
        <StepHeader step={stepper.active} labels={stepper.steps.map((s) => s.label)} />
      )}
      {children}
    </StepRegion>
  ) : (
    children
  )

  const guarded = settings ? (
    <ErrorBoundary scope="feature" name="Storage setup" resetKeys={[region?.stepKey]}>
      {regionBody}
    </ErrorBoundary>
  ) : (
    regionBody
  )

  const formProps = form ? { id: form.id, ref: form.ref, action: form.action, noValidate: true } : null

  if (mobileShell) {
    if (!settings) {
      const screen = (
        <Onboarding title={onboardingTitle} footer={shellFooter}>
          {guarded}
        </Onboarding>
      )

      return formProps ? (
        <form {...formProps} style={{ display: 'contents' }}>
          {screen}
        </form>
      ) : (
        screen
      )
    }

    return (
      <StandardModal
        opened={opened}
        onClose={onClose}
        title={title}
        size="lg"
        fullHeight
        loading={busy}
        dirty={dirty}
        {...(primaryAction ? { primaryAction } : {})}
        {...(onBack ? { backAction: { label: 'Back', onClick: onBack } } : {})}
      >
        {formProps ? <form {...formProps}>{guarded}</form> : guarded}
      </StandardModal>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      closable={closable}
      title={title}
      size="lg"
      loading={busy}
      dirty={dirty}
      footer={footer}
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {stepper && <WizardProgress steps={stepper.steps} active={stepper.active} />}

        <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <FillScrollArea type="auto">
            <Box px="lg" py="lg">
              {formProps ? <form {...formProps}>{guarded}</form> : guarded}
            </Box>
          </FillScrollArea>
        </Box>
      </Stack>
    </StandardModal>
  )
}
