import { Code, Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { CopyButton } from '../../../../components/atoms/CopyButton'

const ENV_VAR_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/

export function buildVaultExportCommand(envVarName: string, password: string): string | null {
  if (!ENV_VAR_NAME.test(envVarName)) {
    return null
  }

  return `export ${envVarName}='${password.replaceAll("'", "'\\''")}'`
}

export function VaultExportCommand({ envVarName, password }: { envVarName: string; password: string }): ReactNode {
  const command = buildVaultExportCommand(envVarName, password)

  return (
    <Group gap="xs" wrap="nowrap" align="center">
      {command ? (
        <>
          <Code block fz="xs" p="xs" style={{ flex: 1, wordBreak: 'break-all' }}>
            {command}
          </Code>
          <CopyButton value={command} label="Copy export command" />
        </>
      ) : (
        <Text size="xs" c="red">
          The variable name {envVarName} is not a valid shell variable name.
        </Text>
      )}
    </Group>
  )
}
