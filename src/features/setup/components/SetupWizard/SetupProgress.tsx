import { Box, Stepper, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useIsMobile } from '../../../../hooks/useIsMobile'

interface SetupProgressProps {
  steps: { label: string; icon: ReactNode }[]
  active: number
}

export function SetupProgress({ steps, active }: SetupProgressProps): ReactNode {
  const { isMobile } = useIsMobile()

  return (
    <Box
      px="lg"
      pt="md"
      pb="md"
      style={{
        flexShrink: 0,
        borderBottom: '1px solid var(--mantine-color-default-border)',
      }}
    >
      <Stepper active={active} size="xs" iconSize={24}>
        {steps.map((s) => (
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
          Step {active + 1} of {steps.length}: {steps[active]?.label}
        </Text>
      )}
    </Box>
  )
}
