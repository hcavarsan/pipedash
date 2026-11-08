import { Box, Group, Stepper, Text } from '@mantine/core'
import { IconCheck, IconKey, IconList, IconPlug } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { iosClasses } from '../../../../theme/ios'
import type { PluginMetadata } from '../../../../types/plugins'
import { STEP_ORDER } from './types'

interface ProviderWizardProgressProps {
  activeStepIndex: number
  isMobile: boolean
  mobileShell: boolean
  selectedPlugin: PluginMetadata | null
  items: string
  onStepSelect: (index: number) => void
}

function ShellDots({ activeStepIndex }: { activeStepIndex: number }): ReactNode {
  return (
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
  )
}

function WizardStepper({
  activeStepIndex,
  isMobile,
  selectedPlugin,
  items,
  onStepSelect,
}: Omit<ProviderWizardProgressProps, 'mobileShell'>): ReactNode {
  const credentialsDescription = selectedPlugin?.presentation.wizard.credentials_description
  return (
    <Stepper
      active={activeStepIndex}
      size={isMobile ? 'xs' : 'sm'}
      iconSize={isMobile ? 32 : 42}
      allowNextStepsSelect={false}
      onStepClick={onStepSelect}
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
        description={isMobile ? undefined : (credentialsDescription ?? 'Enter your API credentials')}
        icon={<IconKey size={18} aria-hidden />}
        completedIcon={<IconCheck size={18} aria-hidden />}
      />
      <Stepper.Step
        aria-label={`Step 3: Select ${items}`}
        label={isMobile ? undefined : `Select ${items}`}
        description={isMobile ? undefined : `Choose ${items} to monitor`}
        icon={<IconList size={18} aria-hidden />}
        completedIcon={<IconCheck size={18} aria-hidden />}
      />
    </Stepper>
  )
}

export function ProviderWizardProgress({ mobileShell, ...stepper }: ProviderWizardProgressProps): ReactNode {
  const { activeStepIndex, isMobile, items } = stepper
  const stepNames = ['Provider', 'Credentials', `Select ${items}`]

  return (
    <Box px="lg" pt={mobileShell ? 0 : 'md'} style={{ flexShrink: 0 }}>
      {mobileShell ? <ShellDots activeStepIndex={activeStepIndex} /> : <WizardStepper {...stepper} />}
      {isMobile && !mobileShell && (
        <Text size="sm" fw={500} ta="center" mt="xs">
          Step {activeStepIndex + 1} of {STEP_ORDER.length}: {stepNames[activeStepIndex]}
        </Text>
      )}
    </Box>
  )
}
