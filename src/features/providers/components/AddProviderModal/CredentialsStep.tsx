import { Box, Input, ScrollArea, SegmentedControl, Select, Stack, Text, TextInput } from '@mantine/core'
import { type ChangeEvent, type Dispatch, Fragment, type ReactNode, type RefObject } from 'react'
import { ProviderIcon } from '../../../../components/atoms/ProviderIcon'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { useFormEnterNavigation } from '../../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { AuthMode, ConfigField, PluginMetadata } from '../../../../types/domain'
import { fieldA11y } from '../../../../utils/formFields'
import { SIGN_IN_KEY, TOKEN_KEY } from '../../lib/credentials'
import { ProviderConfigField } from './ProviderConfigField'
import { ShellField, StepHeading } from './ShellForm'
import { SignInField } from './SignInField'
import type { FormAction, FormState } from './types'
import { useCredentialFields } from './useCredentialFields'
import { activeConfigFields, validateConfigField } from './validation'

const NAME_HINT = 'A friendly name to identify this provider'

interface CredentialsStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  editMode: boolean
  isMobile: boolean
  submitting: boolean
  formRef: RefObject<HTMLFormElement | null>
  formId: string
  permissionRow?: ReactNode | undefined
  secondaryRow?: ReactNode | undefined
  scopeNote?: string | undefined
}

export function CredentialsStep({
  state,
  dispatch,
  plugins,
  pluginsLoading,
  editMode,
  isMobile,
  submitting,
  formRef,
  formId,
  permissionRow,
  secondaryRow,
  scopeNote,
}: CredentialsStepProps): ReactNode {
  useFormEnterNavigation(formRef)
  const mobileShell = useMobileShell()
  const { selectedPlugin, providerName, authMode, configValues, fieldErrors } = state
  const { fields, lastTextKey, fieldStateFor, handleConfigChange, handleConfigBlur, handleNameChange, handleNameBlur } =
    useCredentialFields({ selectedPlugin, providerName, configValues, fieldErrors, dispatch })
  const authModes = selectedPlugin?.config_schema.auth_modes
  const sharedFields = fields.filter((field) => {
    const modes = authModes?.[field.key]

    return !modes || (modes.length > 1 && modes.includes(authMode))
  })
  const modeFields = fields.filter((field) => {
    const modes = authModes?.[field.key]

    return modes?.length === 1 && modes[0] === authMode
  })

  const handlePluginSelect = (providerType: string | null): void => {
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
    enterKeyHint: lastTextKey ? 'next' : 'go',
    name: 'providerName',
    placeholder: selectedPlugin?.provider_type === 'fluxcd' ? 'e.g. FluxCD (production)' : 'e.g. GitHub (work)',
    value: providerName,
    onChange: (event: ChangeEvent<HTMLInputElement>) => handleNameChange(event.currentTarget.value),
    onBlur: handleNameBlur,
    disabled: submitting,
  } as const

  const renderField = (field: ConfigField): ReactNode => (
    <ProviderConfigField
      field={field}
      model={fieldStateFor(field)}
      formId={formId}
      shell={mobileShell}
      disabled={submitting}
      onChange={handleConfigChange}
      onBlur={handleConfigBlur}
    />
  )

  const renderAuthField = (field: ConfigField): ReactNode => {
    const input = renderField(field)

    if (field.key !== TOKEN_KEY) {
      return input
    }

    const scoped = modeFields.map((modeField) => <Fragment key={modeField.key}>{renderField(modeField)}</Fragment>)
    const options: { value: AuthMode; label: string }[] = [
      ...(selectedPlugin?.config_schema.no_auth ? [{ value: 'none' as const, label: 'No auth' }] : []),
      { value: 'token', label: field.label },
      ...(selectedPlugin?.capabilities.sign_in ? [{ value: 'sign_in' as const, label: 'Single sign-on' }] : []),
    ]

    if (!selectedPlugin || options.length < 2) {
      return (
        <>
          {scoped}
          {input}
        </>
      )
    }

    const authControl = (
      <SegmentedControl
        aria-label="Authentication"
        fullWidth
        data={options}
        value={authMode}
        disabled={submitting}
        onChange={(value) => {
          const mode = options.find((option) => option.value === value)?.value

          if (mode) {
            dispatch({ type: 'SET_AUTH_MODE', mode })
          }
        }}
      />
    )
    const noAuthNote = 'Connects without credentials. Read-only: starting and stopping runs is turned off.'
    let body: ReactNode = input

    if (authMode === 'none') {
      body = mobileShell ? (
        <p className={shell.note}>{noAuthNote}</p>
      ) : (
        <Text size="sm" c="dimmed">
          {noAuthNote}
        </Text>
      )
    } else if (authMode === 'sign_in') {
      body = (
        <SignInField
          id={`${formId}-${SIGN_IN_KEY}`}
          providerType={selectedPlugin.provider_type}
          configValues={configValues}
          ready={activeConfigFields(fields, 'sign_in', authModes).every(
            (config) => validateConfigField(config, configValues[config.key] || '') === null
          )}
          error={fieldErrors[SIGN_IN_KEY]}
          disabled={submitting}
          shell={mobileShell}
          onChange={(reference) => dispatch({ type: 'SET_SIGN_IN', reference })}
        />
      )
    }

    return mobileShell ? (
      <>
        <IosGroup header="Authentication">{authControl}</IosGroup>
        {scoped}
        {body}
      </>
    ) : (
      <>
        <Input.Wrapper label="Authentication">
          <Box mt="xs">{authControl}</Box>
        </Input.Wrapper>
        {scoped}
        {body}
      </>
    )
  }

  if (mobileShell) {
    const nameId = `${formId}-providerName`

    return (
      <div className={shell.step}>
        {selectedPlugin ? (
          <>
            <StepHeading
              title="Credentials"
              subtitle={
                selectedPlugin.provider_type === 'fluxcd'
                  ? 'Connect to your Kubernetes cluster'
                  : 'Enter your API credentials'
              }
            />

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

            {sharedFields.map((field) => (
              <Fragment key={field.key}>{renderAuthField(field)}</Fragment>
            ))}

            {permissionRow}
            {scopeNote && <p className={shell.note}>{scopeNote}</p>}
            {secondaryRow}
          </>
        ) : (
          <>
            <StepHeading title="Choose a provider" subtitle="Select the service whose pipelines you want to monitor" />
            {plugins.length > 0 ? (
              <IosGroup>
                {plugins.map((plugin) => (
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
      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={isMobile ? 'xs' : 'md'} px="lg" pb="lg" pt="xs">
          <Select
            label="Provider type"
            placeholder="Select a provider type"
            data={plugins.map((p) => ({ value: p.provider_type, label: p.name }))}
            value={selectedPlugin?.provider_type || null}
            onChange={handlePluginSelect}
            required
            disabled={submitting || editMode}
          />

          {selectedPlugin ? (
            <Stack gap={isMobile ? 'xs' : 'md'}>
              <Text size="sm" c="dimmed">
                {selectedPlugin.description}
              </Text>

              <TextInput
                label="Provider name"
                description={NAME_HINT}
                {...nameProps}
                error={fieldErrors.providerName}
                required
              />

              {sharedFields.map((field) => (
                <Fragment key={field.key}>{renderAuthField(field)}</Fragment>
              ))}
            </Stack>
          ) : (
            !pluginsLoading && (
              <Text size="sm" c="dimmed" ta="center" py="xl">
                Select a provider type to continue
              </Text>
            )
          )}
        </Stack>
      </ScrollArea>
    </Box>
  )
}
