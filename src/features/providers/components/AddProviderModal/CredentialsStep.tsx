import { Box, SegmentedControl, Stack, Text } from '@mantine/core'
import { type Dispatch, Fragment, type ReactNode, type RefObject } from 'react'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup } from '../../../../components/ios/IosList'
import { useFormEnterNavigation } from '../../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { AuthMode, ConfigField, ConfigSection } from '../../../../types/domain'
import { SIGN_IN_KEY, TOKEN_KEY } from '../../lib/credentials'
import { ProviderConfigField } from './ProviderConfigField'
import { StepHeading } from './ShellForm'
import { SignInField } from './SignInField'
import type { FormAction, FormState } from './types'
import { useCredentialFields } from './useCredentialFields'
import { activeConfigFields, STEP_SECTIONS, validateConfigField } from './validation'

const SECTION_TITLES: Partial<Record<ConfigSection, string>> = {
  connection: 'Connection',
  authentication: 'Authentication',
}

interface CredentialsStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
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
  const { fields, fieldStateFor, handleConfigChange, handleConfigBlur } = useCredentialFields({
    selectedPlugin,
    providerName,
    configValues,
    fieldErrors,
    dispatch,
    step: 'credentials',
  })
  const authModes = selectedPlugin?.config_schema.auth_modes
  const tokenField = fields.find((field) => field.key === TOKEN_KEY)
  const visibleFields = fields.filter((field) => authModes?.[field.key]?.includes(authMode) ?? true)
  const inAuthBlock = (field: ConfigField): boolean =>
    authModes?.[field.key]?.length === 1 && field.section === tokenField?.section
  const modeFields = visibleFields.filter(inAuthBlock)
  const sections = STEP_SECTIONS.credentials.map((section) => ({
    section,
    items: visibleFields.filter((field) => !inAuthBlock(field) && field.section === section),
  }))

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
        aria-label="Authentication method"
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
          missing={activeConfigFields(fields, 'sign_in', authModes)
            .filter((config) => validateConfigField(config, configValues[config.key] || '') !== null)
            .map((config) => config.label)}
          error={fieldErrors[SIGN_IN_KEY]}
          disabled={submitting}
          shell={mobileShell}
          onChange={(reference) => dispatch({ type: 'SET_SIGN_IN', reference })}
        />
      )
    }

    return (
      <>
        {authControl}
        {scoped}
        {body}
      </>
    )
  }

  const renderSections = (): ReactNode =>
    sections.map(({ section, items }) => {
      if (items.length === 0) {
        return null
      }

      const headingId = `${formId}-${section}`
      const content = items.map((field) => <Fragment key={field.key}>{renderAuthField(field)}</Fragment>)

      return mobileShell ? (
        <Fragment key={section}>
          <h2 id={headingId} className={shell.sectionTitle}>
            {SECTION_TITLES[section]}
          </h2>
          {content}
        </Fragment>
      ) : (
        <Stack key={section} component="section" gap="sm" aria-labelledby={headingId}>
          <Text id={headingId} component="h3" size="sm" fw={600} m={0}>
            {SECTION_TITLES[section]}
          </Text>
          {content}
        </Stack>
      )
    })

  if (mobileShell) {
    return (
      <div className={shell.step}>
        <StepHeading
          title="Credentials"
          subtitle={
            selectedPlugin?.provider_type === 'fluxcd'
              ? 'Connect to your Kubernetes cluster'
              : 'Enter your API credentials'
          }
        />
        {renderSections()}
        {permissionRow}
        {scopeNote && <p className={shell.note}>{scopeNote}</p>}
        {secondaryRow}
      </div>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <FillScrollArea type="auto">
        <Stack gap={isMobile ? 'lg' : 'xl'} px="lg" pb="lg" pt="xs">
          {renderSections()}
        </Stack>
      </FillScrollArea>
    </Box>
  )
}
