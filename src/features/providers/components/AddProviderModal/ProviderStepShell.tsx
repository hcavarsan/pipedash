import { TextInput, type TextInputProps } from '@mantine/core'
import type { ReactNode } from 'react'
import { ProviderIcon } from '../../../../components/atoms/ProviderIcon'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import type { PluginMetadata } from '../../../../types/plugins'
import { fieldA11y } from '../../../../utils/formFields'
import { ADVANCED_LABEL, AdvancedChevron, type AdvancedSection, NAME_HINT } from './ProviderStepShared'
import { ShellField, StepHeading } from './ShellForm'

interface ShellProviderPickerProps {
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  submitting: boolean
  onSelect: (providerType: string) => void
}

export function ShellProviderPicker({
  plugins,
  pluginsLoading,
  submitting,
  onSelect,
}: ShellProviderPickerProps): ReactNode {
  const rows = plugins.map((plugin) => (
    <IosRow
      key={plugin.provider_type}
      icon={<ProviderIcon name={plugin.name} icon={plugin.icon} size={30} />}
      title={plugin.name}
      subtitle={plugin.description}
      chevron
      onClick={() => onSelect(plugin.provider_type)}
      disabled={submitting}
    />
  ))

  return (
    <>
      <StepHeading title="Choose a provider" subtitle="Select the service whose pipelines you want to monitor" />
      {plugins.length > 0 ? (
        <IosGroup>{rows}</IosGroup>
      ) : (
        !pluginsLoading && <p className={shell.note}>No provider types are available.</p>
      )}
    </>
  )
}

interface ShellProviderDetailsProps {
  plugin: PluginMetadata
  editMode: boolean
  submitting: boolean
  formId: string
  nameProps: TextInputProps
  nameError: string | undefined
  advanced: AdvancedSection
  onClearPlugin: () => void
}

export function ShellProviderDetails({
  plugin,
  editMode,
  submitting,
  formId,
  nameProps,
  nameError,
  advanced,
  onClearPlugin,
}: ShellProviderDetailsProps): ReactNode {
  const nameId = `${formId}-providerName`

  return (
    <>
      <StepHeading title="Provider" subtitle="Give it a name. Credentials come next." />

      <IosGroup header="Provider type" footer={plugin.description}>
        <IosRow
          icon={<ProviderIcon name={plugin.name} icon={plugin.icon} size={30} />}
          title={plugin.name}
          chevron={!editMode}
          {...(editMode ? {} : { onClick: onClearPlugin })}
          disabled={submitting}
        />
      </IosGroup>

      <ShellField id={nameId} label="Provider name" required description={NAME_HINT} error={nameError}>
        <TextInput
          variant="unstyled"
          aria-label="Provider name"
          {...fieldA11y(nameId, { required: true, error: nameError, description: NAME_HINT })}
          {...nameProps}
        />
      </ShellField>

      {advanced.available && (
        <>
          <IosGroup>
            <IosRow
              title={ADVANCED_LABEL}
              accessory={<AdvancedChevron shown={advanced.shown} />}
              onClick={advanced.onToggle}
              disabled={submitting}
            />
          </IosGroup>
          {advanced.shown && advanced.content}
        </>
      )}
    </>
  )
}
