import { Box, Stack, type TextInputProps } from '@mantine/core'
import { type ChangeEvent, type Dispatch, Fragment, type ReactNode, type RefObject, useRef, useState } from 'react'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import shell from '../../../../components/forms/ShellForm.module.css'
import { useFormEnterNavigation } from '../../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { ConfigField, PluginMetadata } from '../../../../types/plugins'
import { ProviderConfigField } from './ProviderConfigField'
import { DesktopProviderDetails, DesktopProviderPicker } from './ProviderStepDesktop'
import { ShellProviderDetails, ShellProviderPicker } from './ProviderStepShell'
import type { FormAction, FormState, ValidateKey } from './types'
import { useCredentialFields } from './useCredentialFields'

interface ProviderStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  plugins: PluginMetadata[]
  editMode: boolean
  isMobile: boolean
  submitting: boolean
  formRef: RefObject<HTMLFormElement | null>
  formId: string
  validateKey: ValidateKey
}

export function ProviderStep({
  state,
  dispatch,
  plugins,
  editMode,
  isMobile,
  submitting,
  formRef,
  formId,
  validateKey,
}: ProviderStepProps): ReactNode {
  useFormEnterNavigation(formRef)
  const mobileShell = useMobileShell()
  const { selectedPlugin, providerName, authMode, configValues, fieldErrors } = state
  const pickedProvider = useRef(false)
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const { fields, lastTextKey, fieldStateFor, handleConfigChange, handleConfigBlur, handleNameChange, handleNameBlur } =
    useCredentialFields({
      selectedPlugin,
      configValues,
      fieldErrors,
      dispatch,
      validateKey,
      step: 'provider',
      authMode,
    })
  const advancedShown = advancedOpen || fields.some((field) => fieldErrors[field.key])

  const handlePluginSelect = (providerType: string | null): void => {
    pickedProvider.current = true
    setAdvancedOpen(false)

    if (!providerType) {
      dispatch({ type: 'CLEAR_PLUGIN' })

      return
    }

    const plugin = plugins.find((p) => p.provider_type === providerType)

    if (plugin) {
      dispatch({ type: 'SELECT_PLUGIN', plugin, isEditMode: editMode })
    }
  }

  const nameProps: TextInputProps = {
    autoCapitalize: 'words',
    autoCorrect: 'off',
    spellCheck: false,
    autoComplete: 'off',
    enterKeyHint: advancedShown && lastTextKey ? 'next' : 'go',
    name: 'providerName',
    placeholder: selectedPlugin?.presentation.wizard.name_placeholder ?? 'Provider name',
    value: providerName,
    onChange: (event: ChangeEvent<HTMLInputElement>) => handleNameChange(event.currentTarget.value),
    onBlur: handleNameBlur,
    disabled: submitting,
  }

  const advanced = {
    available: fields.length > 0,
    shown: advancedShown,
    onToggle: () => setAdvancedOpen(!advancedShown),
    content: fields.map((field: ConfigField) => (
      <Fragment key={field.key}>
        <ProviderConfigField
          field={field}
          model={fieldStateFor(field)}
          formId={formId}
          shell={mobileShell}
          disabled={submitting}
          onChange={handleConfigChange}
          onBlur={handleConfigBlur}
        />
      </Fragment>
    )),
  }

  if (mobileShell) {
    return (
      <div key={selectedPlugin ? 'details' : 'picker'} className={shell.step}>
        {selectedPlugin ? (
          <ShellProviderDetails
            plugin={selectedPlugin}
            editMode={editMode}
            submitting={submitting}
            formId={formId}
            nameProps={nameProps}
            nameError={fieldErrors.providerName}
            advanced={advanced}
            onClearPlugin={() => handlePluginSelect(null)}
          />
        ) : (
          <ShellProviderPicker plugins={plugins} submitting={submitting} onSelect={handlePluginSelect} />
        )}
      </div>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <FillScrollArea type="auto">
        <Stack gap={isMobile ? 'md' : 'lg'} px="lg" pb="lg" pt="xs">
          {selectedPlugin ? (
            <DesktopProviderDetails
              plugin={selectedPlugin}
              editMode={editMode}
              submitting={submitting}
              autoFocus={pickedProvider.current}
              nameProps={nameProps}
              nameError={fieldErrors.providerName}
              advanced={advanced}
              onClearPlugin={() => handlePluginSelect(null)}
            />
          ) : (
            <DesktopProviderPicker
              plugins={plugins}
              submitting={submitting}
              autoFocus={pickedProvider.current}
              onSelect={handlePluginSelect}
            />
          )}
        </Stack>
      </FillScrollArea>
    </Box>
  )
}
