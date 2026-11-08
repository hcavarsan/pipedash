import { Box, Button, Collapse, Stack, Text, TextInput, type TextInputProps } from '@mantine/core'
import { type ReactNode, useId } from 'react'
import type { PluginMetadata } from '../../../../types/plugins'
import { ADVANCED_LABEL, AdvancedChevron, type AdvancedSection, NAME_HINT } from './ProviderStepShared'
import { ProviderTypeCards, SelectedProviderType } from './ProviderTypeCards'

interface DesktopProviderPickerProps {
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  submitting: boolean
  autoFocus: boolean
  onSelect: (providerType: string | null) => void
}

export function DesktopProviderPicker({
  plugins,
  pluginsLoading,
  submitting,
  autoFocus,
  onSelect,
}: DesktopProviderPickerProps): ReactNode {
  return (
    <>
      {plugins.length > 0 && (
        <ProviderTypeCards plugins={plugins} disabled={submitting} autoFocus={autoFocus} onSelect={onSelect} />
      )}
      {plugins.length === 0 && !pluginsLoading && (
        <Text size="sm" c="dimmed" ta="center" py="xl">
          No provider types are available.
        </Text>
      )}
    </>
  )
}

interface DesktopProviderDetailsProps {
  plugin: PluginMetadata
  editMode: boolean
  submitting: boolean
  autoFocus: boolean
  nameProps: TextInputProps
  nameError: string | undefined
  advanced: AdvancedSection
  onClearPlugin: () => void
}

export function DesktopProviderDetails({
  plugin,
  editMode,
  submitting,
  autoFocus,
  nameProps,
  nameError,
  advanced,
  onClearPlugin,
}: DesktopProviderDetailsProps): ReactNode {
  const advancedId = useId()

  return (
    <>
      <SelectedProviderType plugin={plugin} disabled={submitting} onChange={editMode ? undefined : onClearPlugin} />

      <TextInput
        label="Provider name"
        description={NAME_HINT}
        {...nameProps}
        error={nameError}
        required
        autoFocus={autoFocus}
      />

      {advanced.available && (
        <Box>
          <Button
            variant="subtle"
            color="gray"
            size="compact-sm"
            px={4}
            leftSection={<AdvancedChevron shown={advanced.shown} />}
            aria-expanded={advanced.shown}
            aria-controls={advancedId}
            disabled={submitting}
            onClick={advanced.onToggle}
          >
            {ADVANCED_LABEL}
          </Button>
          <Collapse expanded={advanced.shown}>
            <Stack id={advancedId} gap="md" pt="md">
              {advanced.content}
            </Stack>
          </Collapse>
        </Box>
      )}
    </>
  )
}
