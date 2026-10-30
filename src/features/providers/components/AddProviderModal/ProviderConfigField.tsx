import {
  Loader,
  MultiSelect,
  NativeSelect,
  NumberInput,
  PasswordInput,
  Select,
  Switch,
  Textarea,
  TextInput,
} from '@mantine/core'
import type { ChangeEvent, ReactNode } from 'react'
import { SwitchRow } from '../../../../components/ios/mobileParts'
import type { ConfigField } from '../../../../types/domain'
import { fieldA11y, withoutType } from '../../../../utils/formFields'

import { ShellField } from './ShellForm'
import type { CredentialFieldModel } from './useCredentialFields'

interface ProviderConfigFieldProps {
  field: ConfigField
  model: CredentialFieldModel
  formId: string
  shell: boolean
  disabled: boolean
  onChange: (field: ConfigField, value: string) => void
  onBlur: (field: ConfigField) => void
}

type FieldBodyProps = Omit<ProviderConfigFieldProps, 'shell' | 'formId'> & { id: string }

function DesktopField({ field, model, disabled, onChange, onBlur }: FieldBodyProps): ReactNode {
  const { value, options, loadingOptions, error, attributes } = model
  const spinner = loadingOptions ? <Loader size="xs" aria-label={`Loading ${field.label}`} /> : undefined
  const commonProps = {
    value,
    onBlur: () => onBlur(field),
    onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      onChange(field, event.currentTarget.value),
    label: field.label,
    description: field.description ?? undefined,
    error,
    required: field.required,
    disabled,
  }

  switch (field.field_type) {
    case 'TextArea':
      return <Textarea {...commonProps} {...attributes} rows={4} />
    case 'Password':
      return <PasswordInput {...commonProps} {...attributes} />
    case 'Number':
      return (
        <NumberInput
          {...commonProps}
          {...withoutType(attributes)}
          onChange={(val) => onChange(field, String(val ?? ''))}
        />
      )
    case 'Select':
      return (
        <Select
          {...commonProps}
          data={options}
          rightSection={spinner}
          disabled={disabled || loadingOptions}
          onChange={(val) => onChange(field, val || '')}
          searchable
          clearable
        />
      )
    case 'MultiSelect':
      return (
        <MultiSelect
          {...commonProps}
          value={value ? value.split(',').filter(Boolean) : []}
          data={options}
          rightSection={spinner}
          disabled={disabled || loadingOptions}
          onChange={(values) => onChange(field, values.join(','))}
          searchable
          clearable
        />
      )
    case 'Boolean':
      return (
        <Switch
          label={field.label}
          description={field.description ?? undefined}
          checked={value === 'true'}
          error={error}
          required={field.required}
          aria-required={field.required || undefined}
          disabled={disabled}
          onChange={(event) => onChange(field, String(event.currentTarget.checked))}
        />
      )
    default:
      return <TextInput {...commonProps} {...attributes} />
  }
}

function ShellConfigField({ field, model, id, disabled, onChange, onBlur }: FieldBodyProps): ReactNode {
  const { value, options, loadingOptions, error, attributes } = model
  const spinner = loadingOptions ? <Loader size="xs" aria-label={`Loading ${field.label}`} /> : undefined
  const a11y = fieldA11y(id, { required: field.required, error, description: field.description })
  const inputProps = {
    variant: 'unstyled',
    'aria-label': field.label,
    ...a11y,
    value,
    disabled,
    onBlur: () => onBlur(field),
    onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      onChange(field, event.currentTarget.value),
  } as const
  const fieldProps = { id, label: field.label, required: field.required, description: field.description, error }

  switch (field.field_type) {
    case 'Boolean':
      return (
        <ShellField id={id} description={field.description} error={error}>
          <SwitchRow
            title={field.label}
            {...a11y}
            checked={value === 'true'}
            disabled={disabled}
            onChange={(event) => onChange(field, String(event.currentTarget.checked))}
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
            onChange={(val) => onChange(field, String(val ?? ''))}
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
            disabled={disabled || loadingOptions}
            onChange={(event) => onChange(field, event.currentTarget.value)}
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
            disabled={disabled || loadingOptions}
            onChange={(values) => onChange(field, values.join(','))}
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

export function ProviderConfigField({ shell, formId, ...props }: ProviderConfigFieldProps): ReactNode {
  return shell ? (
    <ShellConfigField {...props} id={`${formId}-${props.field.key}`} />
  ) : (
    <DesktopField {...props} id={`${formId}-${props.field.key}`} />
  )
}
