import {
  Box,
  Loader,
  MultiSelect,
  NativeSelect,
  NumberInput,
  PasswordInput,
  ScrollArea,
  Select,
  Stack,
  Switch,
  Text,
  Textarea,
  TextInput,
} from '@mantine/core'
import { useQueries } from '@tanstack/react-query'
import { type Dispatch, Fragment, type ReactNode, type RefObject, useId, useMemo } from 'react'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { useDebounce } from '../../../hooks/useDebounce'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { service } from '../../../services'
import type { ConfigField, PluginMetadata } from '../../../types'
import { type FieldInputKind, fieldA11y, fieldAttributes, withoutType } from '../../../utils/formFields'
import { handleFormEnter } from '../../../utils/formFocus'
import { ProviderIcon } from '../../atoms/ProviderIcon'
import { LiveStatus, useErrorAnnouncement } from '../../forms/LiveStatus'
import { IosGroup, IosRow } from '../../ios/IosList'

import { ShellField, StepHeading } from './ShellForm'
import shell from './ShellForm.module.css'
import type { FormAction, FormState } from './types'
import { validateConfigField, validateProviderName } from './validation'

const NAME_HINT = 'A friendly name to identify this provider'

const INPUT_KINDS: Partial<Record<ConfigField['field_type'], FieldInputKind>> = {
  Password: 'password',
  Number: 'number',
  TextArea: 'textarea',
}

const FORM_STYLE = { display: 'contents' } as const

interface CredentialsStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  editMode: boolean
  isMobile: boolean
  formRef: RefObject<HTMLFormElement | null>
  onSubmit: () => void
  permissionRow?: ReactNode
  secondaryRow?: ReactNode
  scopeNote?: string
}

export function CredentialsStep({
  state,
  dispatch,
  plugins,
  pluginsLoading,
  editMode,
  isMobile,
  formRef,
  onSubmit,
  permissionRow,
  secondaryRow,
  scopeNote,
}: CredentialsStepProps) {
  const mobileShell = useMobileShell()
  const formId = useId()
  const { selectedPlugin, providerName, configValues, fieldErrors, submitting } = state
  const announcement = useErrorAnnouncement(fieldErrors)

  const configValuesKey = useMemo(() => JSON.stringify(configValues), [configValues])
  const debouncedConfigValuesKey = useDebounce(configValuesKey, DEBOUNCE_DELAYS.FILTER)

  const fields = selectedPlugin?.config_schema.fields ?? []
  const lastTextKey = [...fields].reverse().find((field) => field.field_type !== 'Boolean')?.key
  const nameAttributes = {
    autoCapitalize: 'words',
    autoCorrect: 'off',
    spellCheck: false,
    autoComplete: 'off',
    enterKeyHint: lastTextKey ? 'next' : 'go',
  } as const
  const dynamicFields = fields.filter(
    (field) => (field.field_type === 'Select' || field.field_type === 'MultiSelect') && !field.options?.length
  )
  const optionQueries = useQueries({
    queries: dynamicFields.map((field) => ({
      queryKey: ['providerFieldOptions', selectedPlugin?.provider_type, field.key, debouncedConfigValuesKey],
      queryFn: () =>
        service.getProviderFieldOptions(
          selectedPlugin?.provider_type ?? '',
          field.key,
          JSON.parse(debouncedConfigValuesKey) as Record<string, string>
        ),
      enabled: !!selectedPlugin,
      retry: false,
    })),
  })

  const handlePluginSelect = (providerType: string | null) => {
    if (!providerType) {
      dispatch({ type: 'CLEAR_PLUGIN' })

      return
    }

    const plugin = plugins.find((p) => p.provider_type === providerType)

    if (plugin) {
      dispatch({ type: 'SELECT_PLUGIN', plugin, isEditMode: editMode })
    }
  }

  const handleConfigChange = (field: ConfigField, value: string) => {
    dispatch({ type: 'UPDATE_CONFIG', key: field.key, value })

    if (fieldErrors[field.key]) {
      const error = validateConfigField(field, value)

      if (error) {
        dispatch({ type: 'SET_FIELD_ERROR', key: field.key, error })
      }
    }
  }

  const handleConfigBlur = (field: ConfigField) => {
    const error = validateConfigField(field, configValues[field.key] || '')

    if (error) {
      dispatch({ type: 'SET_FIELD_ERROR', key: field.key, error })
    } else if (fieldErrors[field.key]) {
      dispatch({ type: 'CLEAR_FIELD_ERROR', key: field.key })
    }
  }

  const handleNameChange = (name: string) => {
    dispatch({ type: 'SET_PROVIDER_NAME', name })

    if (fieldErrors.providerName) {
      const error = validateProviderName(name)

      if (error) {
        dispatch({ type: 'SET_FIELD_ERROR', key: 'providerName', error })
      }
    }
  }

  const handleNameBlur = () => {
    const error = validateProviderName(providerName)

    if (error) {
      dispatch({ type: 'SET_FIELD_ERROR', key: 'providerName', error })
    } else if (fieldErrors.providerName) {
      dispatch({ type: 'CLEAR_FIELD_ERROR', key: 'providerName' })
    }
  }

  const nameProps = {
    ...nameAttributes,
    placeholder: selectedPlugin?.provider_type === 'fluxcd' ? 'e.g. FluxCD (production)' : 'e.g. GitHub (work)',
    value: providerName,
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => handleNameChange(event.currentTarget.value),
    onBlur: handleNameBlur,
    disabled: submitting,
  }

  const fieldStateFor = (field: ConfigField) => {
    const value = configValues[field.key] || ''
    const fieldOptions = optionQueries[dynamicFields.findIndex((candidate) => candidate.key === field.key)]
    const loadingOptions = !!fieldOptions && (fieldOptions.isFetching || debouncedConfigValuesKey !== configValuesKey)

    return {
      value,
      options: fieldOptions?.data ?? field.options ?? [],
      loadingOptions,
      error: fieldErrors[field.key] || fieldOptions?.error?.message,
      spinner: loadingOptions ? <Loader size="xs" aria-label={`Loading ${field.label}`} /> : undefined,
      attributes: fieldAttributes({
        name: field.key,
        label: field.label,
        kind: INPUT_KINDS[field.field_type] ?? 'text',
        defaultValue: field.default_value,
        last: field.key === lastTextKey,
      }),
      inputProps: {
        value,
        onBlur: () => handleConfigBlur(field),
        onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
          handleConfigChange(field, event.currentTarget.value),
      },
    }
  }

  const renderConfigField = (field: ConfigField) => {
    const { value, options, loadingOptions, error, spinner, attributes, inputProps } = fieldStateFor(field)
    const commonProps = {
      ...inputProps,
      label: field.label,
      description: field.description ?? undefined,
      error,
      required: field.required,
    }

    switch (field.field_type) {
      case 'TextArea':
        return <Textarea key={field.key} {...commonProps} {...attributes} rows={4} />
      case 'Password':
        return <PasswordInput key={field.key} {...commonProps} {...attributes} />
      case 'Number':
        return (
          <NumberInput
            key={field.key}
            {...commonProps}
            {...withoutType(attributes)}
            onChange={(val) => handleConfigChange(field, String(val ?? ''))}
          />
        )
      case 'Select':
        return (
          <Select
            key={field.key}
            {...commonProps}
            data={options}
            rightSection={spinner}
            disabled={submitting || loadingOptions}
            onChange={(val) => handleConfigChange(field, val || '')}
            searchable
            clearable
          />
        )
      case 'MultiSelect':
        return (
          <MultiSelect
            key={field.key}
            {...commonProps}
            value={value ? value.split(',').filter(Boolean) : []}
            data={options}
            rightSection={spinner}
            disabled={submitting || loadingOptions}
            onChange={(values) => handleConfigChange(field, values.join(','))}
            searchable
            clearable
          />
        )
      case 'Boolean':
        return (
          <Switch
            key={field.key}
            label={field.label}
            description={field.description ?? undefined}
            checked={value === 'true'}
            error={fieldErrors[field.key]}
            onChange={(e) => handleConfigChange(field, String(e.currentTarget.checked))}
          />
        )
      default:
        return <TextInput key={field.key} {...commonProps} {...attributes} />
    }
  }

  const renderShellField = (field: ConfigField) => {
    const {
      value,
      options,
      loadingOptions,
      error,
      spinner,
      attributes,
      inputProps: sharedInputProps,
    } = fieldStateFor(field)
    const id = `${formId}-${field.key}`
    const inputProps = {
      variant: 'unstyled',
      'aria-label': field.label,
      ...fieldA11y(id, { required: field.required, error, description: field.description }),
      ...sharedInputProps,
    } as const
    const fieldProps = {
      id,
      label: field.label,
      required: field.required,
      description: field.description,
      error,
    }

    switch (field.field_type) {
      case 'Boolean':
        return (
          <ShellField id={id} description={field.description} error={error}>
            <IosRow
              title={field.label}
              accessory={
                <Switch
                  aria-label={field.label}
                  checked={value === 'true'}
                  onChange={(e) => handleConfigChange(field, String(e.currentTarget.checked))}
                />
              }
            />
          </ShellField>
        )
      case 'TextArea':
        return (
          <ShellField {...fieldProps}>
            <Textarea {...inputProps} {...attributes} autosize minRows={3} />
          </ShellField>
        )
      case 'Password':
        return (
          <ShellField {...fieldProps}>
            <PasswordInput {...inputProps} {...attributes} />
          </ShellField>
        )
      case 'Number':
        return (
          <ShellField {...fieldProps}>
            <NumberInput
              {...inputProps}
              {...withoutType(attributes)}
              hideControls
              onChange={(val) => handleConfigChange(field, String(val ?? ''))}
            />
          </ShellField>
        )
      case 'Select':
        return (
          <ShellField {...fieldProps}>
            <NativeSelect
              {...inputProps}
              data={[{ value: '', label: 'Select' }, ...options]}
              rightSection={spinner}
              disabled={submitting || loadingOptions}
              onChange={(event) => handleConfigChange(field, event.currentTarget.value)}
            />
          </ShellField>
        )
      case 'MultiSelect':
        return (
          <ShellField {...fieldProps}>
            <MultiSelect
              {...inputProps}
              placeholder="Select"
              value={value ? value.split(',').filter(Boolean) : []}
              data={options}
              rightSection={spinner}
              disabled={submitting || loadingOptions}
              onChange={(values) => handleConfigChange(field, values.join(','))}
              clearable
            />
          </ShellField>
        )
      default:
        return (
          <ShellField {...fieldProps}>
            <TextInput {...inputProps} {...attributes} />
          </ShellField>
        )
    }
  }

  const renderContent = () => {
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
                  onClick={editMode ? undefined : () => handlePluginSelect(null)}
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

              {fields.map((field) => (
                <Fragment key={field.key}>{renderShellField(field)}</Fragment>
              ))}

              {permissionRow}
              {scopeNote && <p className={shell.note}>{scopeNote}</p>}
              {secondaryRow}
            </>
          ) : (
            <>
              <StepHeading
                title="Choose a provider"
                subtitle="Select the service whose pipelines you want to monitor"
              />
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
              data={plugins.map((p) => ({
                value: p.provider_type,
                label: p.name,
              }))}
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

                {fields.map((field) => renderConfigField(field))}
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

  return (
    <form
      ref={formRef}
      noValidate
      style={FORM_STYLE}
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit()
      }}
      onKeyDown={handleFormEnter}
    >
      {renderContent()}
      <LiveStatus message={announcement} />
    </form>
  )
}
