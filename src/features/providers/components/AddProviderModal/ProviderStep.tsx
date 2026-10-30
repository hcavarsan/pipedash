import { Box, Button, Collapse, Stack, Text, TextInput } from '@mantine/core'
import { IconChevronRight } from '@tabler/icons-react'
import {
  type ChangeEvent,
  type Dispatch,
  Fragment,
  type ReactNode,
  type RefObject,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react'
import { ProviderIcon } from '../../../../components/atoms/ProviderIcon'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { useFormEnterNavigation } from '../../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { ConfigField, PluginMetadata } from '../../../../types/domain'
import { fieldA11y } from '../../../../utils/formFields'
import { ProviderConfigField } from './ProviderConfigField'
import { ProviderTypeCards, SelectedProviderType } from './ProviderTypeCards'
import { ShellField, StepHeading } from './ShellForm'
import type { FormAction, FormState } from './types'
import { useCredentialFields } from './useCredentialFields'

const NAME_HINT = 'A friendly name to identify this provider'
const ADVANCED_LABEL = 'Advanced settings'

interface ProviderStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  editMode: boolean
  isMobile: boolean
  submitting: boolean
  formRef: RefObject<HTMLFormElement | null>
  formId: string
}

export function ProviderStep({
  state,
  dispatch,
  plugins,
  pluginsLoading,
  editMode,
  isMobile,
  submitting,
  formRef,
  formId,
}: ProviderStepProps): ReactNode {
  useFormEnterNavigation(formRef)
  const mobileShell = useMobileShell()
  const advancedId = useId()
  const { selectedPlugin, providerName, configValues, fieldErrors } = state
  const sortedPlugins = useMemo(() => plugins.toSorted((a, b) => a.name.localeCompare(b.name)), [plugins])
  const pickedProvider = useRef(false)
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const { fields, lastTextKey, fieldStateFor, handleConfigChange, handleConfigBlur, handleNameChange, handleNameBlur } =
    useCredentialFields({ selectedPlugin, providerName, configValues, fieldErrors, dispatch, step: 'provider' })
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

  const nameProps = {
    autoCapitalize: 'words',
    autoCorrect: 'off',
    spellCheck: false,
    autoComplete: 'off',
    enterKeyHint: advancedShown && lastTextKey ? 'next' : 'go',
    name: 'providerName',
    placeholder: selectedPlugin?.provider_type === 'fluxcd' ? 'e.g. FluxCD (production)' : 'e.g. GitHub (work)',
    value: providerName,
    onChange: (event: ChangeEvent<HTMLInputElement>) => handleNameChange(event.currentTarget.value),
    onBlur: handleNameBlur,
    disabled: submitting,
  } as const

  const renderFields = (): ReactNode =>
    fields.map((field: ConfigField) => (
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
    ))

  const advancedIcon = (
    <IconChevronRight
      size={16}
      aria-hidden
      style={{ transform: advancedShown ? 'rotate(90deg)' : undefined, transition: 'transform 150ms ease' }}
    />
  )

  if (mobileShell) {
    const nameId = `${formId}-providerName`

    return (
      <div key={selectedPlugin ? 'details' : 'picker'} className={shell.step}>
        {selectedPlugin ? (
          <>
            <StepHeading title="Provider" subtitle="Give it a name. Credentials come next." />

            <IosGroup header="Provider type" footer={selectedPlugin.description}>
              <IosRow
                icon={<ProviderIcon name={selectedPlugin.name} icon={selectedPlugin.icon} size={30} />}
                title={selectedPlugin.name}
                chevron={!editMode}
                {...(editMode ? {} : { onClick: () => handlePluginSelect(null) })}
                disabled={submitting}
              />
            </IosGroup>

            <ShellField
              id={nameId}
              label="Provider name"
              required
              description={NAME_HINT}
              error={fieldErrors.providerName}
            >
              <TextInput
                variant="unstyled"
                aria-label="Provider name"
                {...fieldA11y(nameId, { required: true, error: fieldErrors.providerName, description: NAME_HINT })}
                {...nameProps}
              />
            </ShellField>

            {fields.length > 0 && (
              <>
                <IosGroup>
                  <IosRow
                    title={ADVANCED_LABEL}
                    accessory={advancedIcon}
                    onClick={() => setAdvancedOpen(!advancedShown)}
                    disabled={submitting}
                  />
                </IosGroup>
                {advancedShown && renderFields()}
              </>
            )}
          </>
        ) : (
          <>
            <StepHeading title="Choose a provider" subtitle="Select the service whose pipelines you want to monitor" />
            {plugins.length > 0 ? (
              <IosGroup>
                {sortedPlugins.map((plugin) => (
                  <IosRow
                    key={plugin.provider_type}
                    icon={<ProviderIcon name={plugin.name} icon={plugin.icon} size={30} />}
                    title={plugin.name}
                    subtitle={plugin.description}
                    chevron
                    onClick={() => handlePluginSelect(plugin.provider_type)}
                    disabled={submitting}
                  />
                ))}
              </IosGroup>
            ) : (
              !pluginsLoading && <p className={shell.note}>No provider types are available.</p>
            )}
          </>
        )}
      </div>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <FillScrollArea type="auto">
        <Stack gap={isMobile ? 'md' : 'lg'} px="lg" pb="lg" pt="xs">
          {selectedPlugin ? (
            <>
              <SelectedProviderType
                plugin={selectedPlugin}
                disabled={submitting}
                onChange={editMode ? undefined : () => handlePluginSelect(null)}
              />

              <TextInput
                label="Provider name"
                description={NAME_HINT}
                {...nameProps}
                error={fieldErrors.providerName}
                required
                autoFocus={pickedProvider.current}
              />

              {fields.length > 0 && (
                <Box>
                  <Button
                    variant="subtle"
                    color="gray"
                    size="compact-sm"
                    px={4}
                    leftSection={advancedIcon}
                    aria-expanded={advancedShown}
                    aria-controls={advancedId}
                    disabled={submitting}
                    onClick={() => setAdvancedOpen(!advancedShown)}
                  >
                    {ADVANCED_LABEL}
                  </Button>
                  <Collapse expanded={advancedShown}>
                    <Stack id={advancedId} gap="md" pt="md">
                      {renderFields()}
                    </Stack>
                  </Collapse>
                </Box>
              )}
            </>
          ) : sortedPlugins.length > 0 ? (
            <ProviderTypeCards
              plugins={sortedPlugins}
              disabled={submitting}
              autoFocus={pickedProvider.current}
              onSelect={handlePluginSelect}
            />
          ) : (
            !pluginsLoading && (
              <Text size="sm" c="dimmed" ta="center" py="xl">
                No provider types are available.
              </Text>
            )
          )}
        </Stack>
      </FillScrollArea>
    </Box>
  )
}
