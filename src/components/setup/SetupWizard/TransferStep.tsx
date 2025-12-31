import { Alert, Box, Group, Radio, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconDatabase, IconSparkles } from '@tabler/icons-react'

import type { TransferStepProps } from './types'

export function TransferStep({
  state,
  setState,
  error,
}: Omit<TransferStepProps, 'isDatabaseBackendChange' | 'isDataDirChange' | 'isFromKeyring' | 'getCurrentBackend' | 'getCurrentDataDir'>) {
  return (
    <Stack gap="md">
      <Radio.Group
        value={state.transferData ? 'transfer' : 'fresh'}
        onChange={(value) => setState({ ...state, transferData: value === 'transfer' })}
      >
        <Stack gap="sm">
          <Box
            onClick={() => setState({ ...state, transferData: true })}
            p="md"
            style={{
              cursor: 'pointer',
              borderRadius: 'var(--mantine-radius-md)',
              border: `1px solid ${state.transferData ? 'var(--mantine-color-blue-5)' : 'var(--mantine-color-dark-4)'}`,
              backgroundColor: state.transferData ? 'rgba(34, 139, 230, 0.08)' : 'transparent',
              transition: 'all 0.15s ease',
            }}
          >
            <Group gap="md" wrap="nowrap" align="flex-start">
              <Radio
                value="transfer"
                styles={{
                  radio: { cursor: 'pointer' },
                }}
              />
              <Stack gap={4} style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconDatabase
                    size={16}
                    style={{
                      color: state.transferData ? 'var(--mantine-color-blue-5)' : 'var(--mantine-color-dimmed)',
                    }}
                  />
                  <Text size="sm" fw={500}>
                    Transfer Data
                  </Text>
                  <Text size="xs" c="blue" fw={500}>
                    Recommended
                  </Text>
                </Group>
                <Text size="xs" c="dimmed">
                  Migrate providers, tokens, and history
                </Text>
              </Stack>
            </Group>
          </Box>

          <Box
            onClick={() => setState({ ...state, transferData: false })}
            p="md"
            style={{
              cursor: 'pointer',
              borderRadius: 'var(--mantine-radius-md)',
              border: `1px solid ${!state.transferData ? 'var(--mantine-color-blue-5)' : 'var(--mantine-color-dark-4)'}`,
              backgroundColor: !state.transferData ? 'rgba(34, 139, 230, 0.08)' : 'transparent',
              transition: 'all 0.15s ease',
            }}
          >
            <Group gap="md" wrap="nowrap" align="flex-start">
              <Radio
                value="fresh"
                styles={{
                  radio: { cursor: 'pointer' },
                }}
              />
              <Stack gap={4} style={{ flex: 1 }}>
                <Group gap="xs" wrap="nowrap">
                  <IconSparkles
                    size={16}
                    style={{
                      color: !state.transferData ? 'var(--mantine-color-blue-5)' : 'var(--mantine-color-dimmed)',
                    }}
                  />
                  <Text size="sm" fw={500}>
                    Fresh Start
                  </Text>
                </Group>
                <Text size="xs" c="dimmed">
                  Start with empty database
                </Text>
              </Stack>
            </Group>
          </Box>
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
