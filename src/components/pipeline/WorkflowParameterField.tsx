import { NativeSelect, NumberInput, Select, Switch, Text, TextInput } from '@mantine/core'
import type { ReactNode } from 'react'

import type { WorkflowParameter } from '../../types'
import { errorId, fieldA11y, fieldAttributes, helpId, withoutType } from '../../utils/formFields'
import { IosRow } from '../ios/IosList'
import shell from '../provider/AddProviderModal/ShellForm.module.css'
import { comparable } from './triggerParams'

interface WorkflowParameterFieldProps {
  param: WorkflowParameter
  value: unknown
  error: string | undefined
  formId: string
  last: boolean
  mobileShell: boolean
  pending: boolean
  onChange: (value: unknown) => void
  onBlur: () => void
}

const rightAligned = {
  input: {
    minHeight: '2em',
    paddingInline: 0,
    background: 'transparent',
    textAlign: 'right' as const,
  },
}

const choiceOptions = (param: WorkflowParameter): string[] =>
  param.type === 'choice' ? [...new Set(param.options)] : []

function ShellField({ param, value, error, formId, last, pending, onChange, onBlur }: WorkflowParameterFieldProps) {
  const label = param.label || param.name
  const id = `${formId}-${param.name}`
  const a11y = fieldA11y(id, { required: param.required, error, description: param.description })
  const attributes = fieldAttributes({
    name: param.name,
    label: param.label,
    kind: param.type === 'number' ? 'number' : 'text',
    defaultValue: param.default,
    last,
  })
  const title = (
    <>
      {label}
      {param.required && (
        <Text component="span" inherit c="red" aria-hidden>
          {' *'}
        </Text>
      )}
    </>
  )
  const subtitle =
    error || param.description ? (
      <>
        {error && (
          <span id={errorId(id)} className={`${shell.line} ${shell.error}`}>
            {error}
          </span>
        )}
        {param.description && (
          <span id={helpId(id)} className={shell.line}>
            {param.description}
          </span>
        )}
      </>
    ) : undefined
  const placeholder = param.required ? 'Required' : 'Optional'
  let accessory: ReactNode

  switch (param.type) {
    case 'boolean':
      accessory = (
        <Switch
          aria-label={label}
          {...a11y}
          checked={Boolean(value)}
          onChange={(event) => onChange(event.currentTarget.checked)}
          disabled={pending}
        />
      )
      break

    case 'choice':
      accessory = (
        <NativeSelect
          aria-label={label}
          {...a11y}
          disabled={pending}
          onBlur={onBlur}
          variant="unstyled"
          data={[{ value: '', label: param.required ? 'Select' : 'None' }, ...choiceOptions(param)]}
          value={typeof value === 'string' ? value : ''}
          onChange={(event) => onChange(event.currentTarget.value || null)}
          w="8em"
          rightSectionWidth={20}
          styles={{ input: { ...rightAligned.input, paddingRight: '1.4em' } }}
        />
      )
      break

    case 'number':
      accessory = (
        <NumberInput
          aria-label={label}
          {...a11y}
          {...withoutType(attributes)}
          disabled={pending}
          onBlur={onBlur}
          value={typeof value === 'number' || typeof value === 'string' ? value : ''}
          onChange={onChange}
          variant="unstyled"
          hideControls
          placeholder={placeholder}
          w="6em"
          styles={rightAligned}
        />
      )
      break

    default:
      accessory = (
        <TextInput
          aria-label={label}
          {...a11y}
          {...attributes}
          disabled={pending}
          onBlur={onBlur}
          value={comparable(value)}
          onChange={(event) => onChange(event.currentTarget.value)}
          variant="unstyled"
          placeholder={placeholder}
          w="8em"
          styles={rightAligned}
        />
      )
  }

  return <IosRow title={title} subtitle={subtitle} accessory={accessory} />
}

function DesktopField({ param, value, error, last, pending, onChange, onBlur }: WorkflowParameterFieldProps) {
  const label = param.label || param.name
  const description = param.description || undefined
  const attributes = fieldAttributes({
    name: param.name,
    label: param.label,
    kind: param.type === 'number' ? 'number' : 'text',
    defaultValue: param.default,
    last,
  })

  switch (param.type) {
    case 'boolean':
      return (
        <Switch
          label={label}
          description={description}
          checked={Boolean(value)}
          onChange={(event) => onChange(event.currentTarget.checked)}
          disabled={pending}
          required={param.required}
          error={error}
        />
      )

    case 'choice': {
      const options = choiceOptions(param)

      return (
        <Select
          label={label}
          description={description}
          placeholder="Select an option"
          disabled={pending}
          onBlur={onBlur}
          data={options}
          value={typeof value === 'string' && value !== '' && options.includes(value) ? value : null}
          onChange={onChange}
          clearable={!param.required}
          error={error}
          required={param.required}
        />
      )
    }

    case 'number':
      return (
        <NumberInput
          {...withoutType(attributes)}
          disabled={pending}
          onBlur={onBlur}
          value={typeof value === 'number' || typeof value === 'string' ? value : ''}
          onChange={onChange}
          label={label}
          description={description}
          placeholder="Enter a number"
          error={error}
          required={param.required}
        />
      )

    default:
      return (
        <TextInput
          {...attributes}
          disabled={pending}
          onBlur={onBlur}
          value={comparable(value)}
          onChange={(event) => onChange(event.currentTarget.value)}
          label={label}
          description={description}
          placeholder={`Enter ${label.toLowerCase()}`}
          error={error}
          required={param.required}
        />
      )
  }
}

export function WorkflowParameterField(props: WorkflowParameterFieldProps): ReactNode {
  return props.mobileShell ? <ShellField {...props} /> : <DesktopField {...props} />
}
