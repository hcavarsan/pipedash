import { Divider, Group, Paper, Stack, Text, Title } from '@mantine/core'
import { Fragment, type ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { DisplayField } from '../../../types/presentation'

interface RunInputsProps {
  inputs?: DisplayField[]
  title: string
  mobileShell: boolean
  isMobile: boolean
}

export function RunInputs({ inputs = [], title, mobileShell, isMobile }: RunInputsProps): ReactNode {
  if (inputs.length === 0) {
    return null
  }

  if (mobileShell) {
    return (
      <div data-selectable>
        <IosGroup header={title}>
          {inputs.map(({ label, value }) =>
            value.length > 32 ? (
              <IosRow key={label} title={value} subtitle={label} />
            ) : (
              <IosRow key={label} title={label} value={value} />
            )
          )}
        </IosGroup>
      </div>
    )
  }

  return (
    <Paper withBorder p={isMobile ? 'md' : 'lg'} component="section" aria-label={title}>
      <Stack gap="sm">
        <Title order={3} size="sm">
          {title}
        </Title>
        {inputs.map((input) => (
          <Fragment key={input.label}>
            <Divider />
            <DesktopInputRow input={input} isMobile={isMobile} />
          </Fragment>
        ))}
      </Stack>
    </Paper>
  )
}

function DesktopInputRow({ input, isMobile }: { input: DisplayField; isMobile: boolean }): ReactNode {
  return (
    <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
      <Text
        size={TYPOGRAPHY.FIELD_VALUE.size}
        c="dimmed"
        w={isMobile ? 110 : 160}
        style={{ flex: 'none', overflowWrap: 'anywhere' }}
      >
        {input.label}
      </Text>
      <Text size={TYPOGRAPHY.FIELD_VALUE.size} flex={1} miw={0} style={{ overflowWrap: 'anywhere' }}>
        {input.value}
      </Text>
    </Group>
  )
}
