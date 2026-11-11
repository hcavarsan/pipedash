import { Alert, Radio, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconDatabase, IconDatabaseOff, type TablerIcon } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup } from '../../../../components/ios/IosList'
import { useMobileShell } from '../../../../hooks/useIsMobile'

import { TransferMobileOption } from './TransferMobileOption'
import { TransferOptionCard } from './TransferOptionCard'
import type { TransferStepProps } from './types'

interface TransferOption {
  transfer: boolean
  value: string
  title: string
  description: string
  recommended: boolean
  Icon: TablerIcon
}

const OPTIONS: TransferOption[] = [
  {
    transfer: true,
    value: 'transfer',
    title: 'Transfer data',
    description: 'Migrate providers, tokens, and history to the new location.',
    recommended: true,
    Icon: IconDatabase,
  },
  {
    transfer: false,
    value: 'fresh',
    title: 'Start fresh',
    description:
      'Start with an empty database. Existing providers and credentials stay behind in the previous location.',
    recommended: false,
    Icon: IconDatabaseOff,
  },
]

export function TransferStep({ state, setState, error }: TransferStepProps): ReactNode {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <Stack gap="md">
        <IosGroup header="Existing data">
          {OPTIONS.map((option) => (
            <TransferMobileOption
              key={option.title}
              Icon={option.Icon}
              title={option.title}
              subtitle={option.recommended ? `Recommended. ${option.description}` : option.description}
              selected={state.transferData === option.transfer}
              onSelect={() => setState({ ...state, transferData: option.transfer })}
            />
          ))}
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
          {OPTIONS.map((option) => (
            <TransferOptionCard
              key={option.value}
              value={option.value}
              Icon={option.Icon}
              title={option.title}
              description={option.description}
              recommended={option.recommended}
            />
          ))}
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
