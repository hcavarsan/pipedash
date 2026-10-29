import { Alert, Group, Radio, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconDatabase, IconDatabaseOff } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { IosGroup, IosRow } from '../../ios/IosList'

import type { TransferStepProps } from './types'

export function TransferStep({ state, setState, error }: TransferStepProps): ReactNode {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    const options = [
      {
        transfer: true,
        title: 'Transfer data',
        subtitle: 'Recommended. Migrate providers, tokens, and history to the new location.',
        icon: <IconDatabase size={18} aria-hidden />,
      },
      {
        transfer: false,
        title: 'Start fresh',
        subtitle:
          'Start with an empty database. Existing providers and credentials stay behind in the previous location.',
        icon: <IconDatabaseOff size={18} aria-hidden />,
      },
    ]

    return (
      <Stack gap="md">
        <IosGroup header="Existing data">
          {options.map((option) => {
            const selected = state.transferData === option.transfer

            return (
              <IosRow
                key={option.title}
                icon={option.icon}
                title={option.title}
                subtitle={option.subtitle}
                selected={selected}
                accessory={
                  selected ? (
                    <IconCheck size={20} color="var(--mantine-color-blue-light-color)" aria-hidden />
                  ) : undefined
                }
                onClick={() => setState({ ...state, transferData: option.transfer })}
              />
            )
          })}
        </IosGroup>

        {error && (
          <Text size="sm" c="red" px="md" role="alert">
            {error}
          </Text>
        )}
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      <Radio.Group
        label="Existing data"
        value={state.transferData ? 'transfer' : 'fresh'}
        onChange={(value) => setState({ ...state, transferData: value === 'transfer' })}
      >
        <Stack gap="sm" mt="xs">
          <Radio.Card value="transfer" p="md" radius="md">
            <Group gap="md" wrap="nowrap" align="flex-start">
              <Radio.Indicator />
              <Stack gap="3xs" style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconDatabase size={16} aria-hidden />
                  <Text size="sm" fw={500}>
                    Transfer data
                  </Text>
                  <Text size="xs" c="blue" fw={500}>
                    Recommended
                  </Text>
                </Group>
                <Text size="xs" c="dimmed">
                  Migrate providers, tokens, and history to the new location.
                </Text>
              </Stack>
            </Group>
          </Radio.Card>

          <Radio.Card value="fresh" p="md" radius="md">
            <Group gap="md" wrap="nowrap" align="flex-start">
              <Radio.Indicator />
              <Stack gap="3xs" style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconDatabaseOff size={16} aria-hidden />
                  <Text size="sm" fw={500}>
                    Start fresh
                  </Text>
                </Group>
                <Text size="xs" c="dimmed">
                  Start with an empty database. Existing providers and credentials stay behind in the previous location.
                </Text>
              </Stack>
            </Group>
          </Radio.Card>
        </Stack>
      </Radio.Group>

      {!state.transferData && (
        <Alert
          icon={<IconAlertCircle size={16} aria-hidden />}
          color="orange"
          title="Switching without transferring data"
        >
          Existing providers, saved credentials, and pipeline history will stay in the previous location and will not be
          available after switching. You will need to configure your providers again.
        </Alert>
      )}

      {error && (
        <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="red" title="Couldn't transfer data">
          {error}
        </Alert>
      )}
    </Stack>
  )
}
