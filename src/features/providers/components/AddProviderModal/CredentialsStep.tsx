import { Box, Stack, Text } from '@mantine/core'
import { type Dispatch, Fragment, type ReactNode, type RefObject } from 'react'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import shell from '../../../../components/forms/ShellForm.module.css'
import { useFormEnterNavigation } from '../../../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { ConfigSection } from '../../../../types/plugins'
import { authOptions } from '../../lib/authModes'
import { SIGN_IN_KEY } from '../../lib/credentials'
import { AuthMethodPicker } from './AuthMethodPicker'
import { ProviderConfigField } from './ProviderConfigField'
import { StepHeading } from './ShellForm'
import { SignInField } from './SignInField'
import type { FormAction, FormState, ValidateKey } from './types'
import { useCredentialFields } from './useCredentialFields'
import { STEP_SECTIONS } from './validation'

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
  validateKey: ValidateKey
  missingLabels: string[]
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
  validateKey,
  missingLabels,
  permissionRow,
  secondaryRow,
  scopeNote,
}: CredentialsStepProps): ReactNode {
  useFormEnterNavigation(formRef)
  const mobileShell = useMobileShell()
  const { selectedPlugin, authMode, configValues, fieldErrors } = state
  const { fields, fieldStateFor, handleConfigChange, handleConfigBlur } = useCredentialFields({
    selectedPlugin,
    configValues,
    fieldErrors,
    dispatch,
    validateKey,
    step: 'credentials',
    authMode,
  })
  const options = authOptions(selectedPlugin)
  const sections = STEP_SECTIONS.credentials.map((section) => ({
    section,
    items: fields.filter((field) => field.section === section),
  }))

  const renderSections = (): ReactNode =>
    sections.map(({ section, items }) => {
      const signIn = section === 'authentication' && authMode === 'sign_in' && selectedPlugin !== null

      if (items.length === 0 && !signIn) {
        return null
      }

      const headingId = `${formId}-${section}`
      const content = (
        <>
          {items.map((field) => (
            <ProviderConfigField
              key={field.key}
              field={field}
              model={fieldStateFor(field)}
              formId={formId}
              shell={mobileShell}
              disabled={submitting}
              onChange={handleConfigChange}
              onBlur={handleConfigBlur}
            />
          ))}
          {signIn && (
            <SignInField
              id={`${formId}-${SIGN_IN_KEY}`}
              providerType={selectedPlugin.provider_type}
              configValues={configValues}
              missing={missingLabels}
              error={fieldErrors[SIGN_IN_KEY]}
              disabled={submitting}
              shell={mobileShell}
              onChange={(reference) => dispatch({ type: 'SET_SIGN_IN', reference })}
            />
          )}
        </>
      )

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

  const renderBody = (): ReactNode => (
    <>
      {options.length > 1 && (
        <AuthMethodPicker
          options={options}
          value={authMode}
          disabled={submitting}
          shell={mobileShell}
          headingId={`${formId}-auth-method`}
          onChange={(mode) => dispatch({ type: 'SET_AUTH_MODE', mode })}
        />
      )}
      {renderSections()}
    </>
  )

  if (mobileShell) {
    return (
      <div className={shell.step}>
        <StepHeading title="Credentials" subtitle={selectedPlugin?.presentation.wizard.credentials_description ?? ''} />
        {renderBody()}
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
          {renderBody()}
        </Stack>
      </FillScrollArea>
    </Box>
  )
}
