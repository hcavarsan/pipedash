import { Box, Stack, Stepper, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import type { ReactNode, RefObject } from 'react'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import { type ModalAction, StandardModal } from '../../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../../components/ErrorBoundary/ErrorBoundary'
import { StepRegion } from '../../../../components/forms/StepRegion'
import { useIsMobile, useMobileShell } from '../../../../hooks/useIsMobile'

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
  footer: ReactNode
  shellFooter: ReactNode
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
  shellFooter,
  primaryAction,
  onBack,
  children,
}: SetupWizardLayoutProps): ReactNode {
  const { isMobile } = useIsMobile()
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
      footer={footer}
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {stepper && (
          <Box
            px="lg"
            pt="md"
            pb="md"
            style={{
              flexShrink: 0,
              borderBottom: '1px solid var(--mantine-color-default-border)',
            }}
          >
            <Stepper active={stepper.active} size="xs" iconSize={24}>
              {stepper.steps.map((s) => (
                <Stepper.Step
                  key={s.label}
                  label={isMobile ? undefined : s.label}
                  aria-label={s.label}
                  icon={s.icon}
                  completedIcon={<IconCheck size={16} aria-hidden />}
                />
              ))}
            </Stepper>

            {isMobile && (
              <Text size="xs" c="dimmed" ta="center" mt="xs">
                Step {stepper.active + 1} of {stepper.steps.length}: {stepper.steps[stepper.active]?.label}
              </Text>
            )}
          </Box>
        )}

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
