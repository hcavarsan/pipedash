import { Alert, Group, Radio, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconDatabase, IconDatabaseOff } from '@tabler/icons-react'

import type { TransferStepProps } from './types'

export function TransferStep({ state, setState, error }: TransferStepProps) {
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
              <Stack gap={4} style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconDatabase size={16} />
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
              <Stack gap={4} style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconDatabaseOff size={16} />
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

      {error && (
        <Alert icon={<IconAlertCircle size={16} />} color="red">
          {error}
        </Alert>
      )}
    </Stack>
  )
}
