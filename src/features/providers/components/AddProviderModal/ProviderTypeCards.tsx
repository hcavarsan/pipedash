import { Box, Button, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { type ReactNode, useId } from 'react'

import { ProviderIcon } from '../../../../components/atoms/ProviderIcon'
import type { PluginMetadata } from '../../../../types/domain'

import classes from './ProviderTypeCards.module.css'

interface ProviderTypeCardsProps {
  plugins: PluginMetadata[]
  disabled: boolean
  autoFocus: boolean
  onSelect: (providerType: string) => void
}

export function ProviderTypeCards({ plugins, disabled, autoFocus, onSelect }: ProviderTypeCardsProps): ReactNode {
  const id = useId()

  return (
    <Stack gap="xs">
      <Text id={`${id}-label`} size="sm" fw={500}>
        Provider type
      </Text>
      <SimpleGrid role="group" aria-labelledby={`${id}-label`} cols={{ base: 1, xs: 2, md: 3 }} spacing="sm">
        {plugins.map((plugin, index) => (
          <UnstyledButton
            key={plugin.provider_type}
            className={`${classes.card} interactive-surface`}
            aria-label={plugin.name}
            aria-describedby={`${id}-${plugin.provider_type}`}
            disabled={disabled}
            autoFocus={autoFocus && index === 0}
            onClick={() => onSelect(plugin.provider_type)}
          >
            <ProviderIcon name={plugin.name} icon={plugin.icon} size={36} />
            <Box style={{ minWidth: 0 }}>
              <Text size="sm" fw={600} truncate>
                {plugin.name}
              </Text>
              <Text id={`${id}-${plugin.provider_type}`} size="xs" c="dimmed" lineClamp={2}>
                {plugin.description}
              </Text>
            </Box>
          </UnstyledButton>
        ))}
      </SimpleGrid>
    </Stack>
  )
}

interface SelectedProviderTypeProps {
  plugin: PluginMetadata
  disabled: boolean
  onChange: (() => void) | undefined
}

export function SelectedProviderType({ plugin, disabled, onChange }: SelectedProviderTypeProps): ReactNode {
  return (
    <Stack gap="xs">
      <Text size="sm" fw={500}>
        Provider type
      </Text>
      <div className={classes.selected}>
        <ProviderIcon name={plugin.name} icon={plugin.icon} size={36} />
        <Box style={{ flex: 1, minWidth: 0 }}>
          <Text size="sm" fw={600}>
            {plugin.name}
          </Text>
          <Text size="xs" c="dimmed">
            {plugin.description}
          </Text>
        </Box>
        {onChange && (
          <Button variant="default" size="xs" disabled={disabled} onClick={onChange}>
            Change
          </Button>
        )}
      </div>
    </Stack>
  )
}
