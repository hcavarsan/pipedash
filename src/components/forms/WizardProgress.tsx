import { Box, Group, Stepper, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { iosClasses } from '../../theme/ios'

interface WizardProgressStep {
  label: string
  description?: string
  icon: ReactNode
}

interface WizardProgressProps {
  steps: WizardProgressStep[]
  active: number
  onStepSelect?: ((index: number) => void) | undefined
}

export function WizardProgress({ steps, active, onStepSelect }: WizardProgressProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <Group
        className={iosClasses.stepperSteps}
        gap="2xs"
        justify="center"
        role="img"
        aria-label={`Step ${active + 1} of ${steps.length}`}
      >
        {steps.map((step, index) => (
          <Box
            key={step.label}
            className={iosClasses.stepperStepIcon}
            data-progress={index <= active || undefined}
            aria-hidden
          />
        ))}
      </Group>
    )
  }

  return (
    <Box px="lg" py="md" style={{ flexShrink: 0, borderBottom: '1px solid var(--mantine-color-default-border)' }}>
      <Stepper
        active={active}
        size="xs"
        iconSize={24}
        allowNextStepsSelect={false}
        {...(onStepSelect ? { onStepClick: onStepSelect } : {})}
      >
        {steps.map((step, index) => (
          <Stepper.Step
            key={step.label}
            label={isMobile ? undefined : step.label}
            description={isMobile ? undefined : step.description}
            aria-label={`Step ${index + 1}: ${step.label}`}
            icon={step.icon}
            completedIcon={<IconCheck size={16} aria-hidden />}
          />
        ))}
      </Stepper>

      {isMobile && (
        <Text size="xs" c="dimmed" ta="center" mt="xs">
          Step {active + 1} of {steps.length}: {steps[active]?.label}
        </Text>
      )}
    </Box>
  )
}
