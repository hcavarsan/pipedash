import { Button, Group, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { parse } from 'smol-toml'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useMigrateAndSaveConfig, useSaveConfigContent } from '../../queries/useStorageQueries'
import type { ConfigAnalysisResponse, MigrationOptions, MigrationPlan } from '../../types'
import { openConfirm } from '../../utils/confirm'
import { errorMessage } from '../../utils/errorDisplay'
import { formatCount } from '../../utils/formatCount'
import { StandardModal } from '../common/StandardModal'

import { ConfigAnalysisNotices } from './ConfigAnalysisNotices'
import { ConfigEditorField } from './ConfigEditorField'
import { MigrationConfirmModal } from './MigrationConfirmModal'
import { useAnalyzeConfigNow, useConfigAnalysis } from './useConfigAnalysis'

interface ConfigEditorModalProps {
  onClose: () => void
  initialContent: string
  onSaved?: () => void
}

type Submission =
  | { kind: 'migration'; content: string; plan: MigrationPlan; analysis: ConfigAnalysisResponse }
  | { kind: 'warnings' }

interface EditorActionState {
  forContent: string | null
  saveError: string | null
  analysisError: string | null
}

const IDLE_STATE: EditorActionState = { forContent: null, saveError: null, analysisError: null }

const FORM_STYLE = { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, overflow: 'hidden' } as const

function validateConfigToml(content: string): string | null {
  if (!content.trim()) {
    return 'Config file cannot be empty'
  }

  try {
    parse(content)

    return null
  } catch (error) {
    return error instanceof Error ? error.message : 'Invalid TOML syntax'
  }
}

export const ConfigEditorModal = ({ onClose, initialContent, onSaved }: ConfigEditorModalProps): ReactNode => {
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const [content, setContent] = useState(initialContent)
  const [submission, setSubmission] = useState<Submission | null>(null)
  const saveConfig = useSaveConfigContent()
  const migrateAndSave = useMigrateAndSaveConfig()
  const analyzeNow = useAnalyzeConfigNow()

  const hasChanges = content !== initialContent
  const localError = validateConfigToml(content)
  const background = useConfigAnalysis(content, hasChanges && localError === null)
  const analysis = background.data ?? null
  const serverError = analysis && !analysis.valid ? (analysis.errors[0]?.message ?? null) : null
  const validationError = localError ?? serverError

  const persist = async (submitted: string): Promise<string | null> => {
    try {
      await saveConfig.mutateAsync(submitted)
      notifications.show({
        title: 'Configuration saved',
        message: 'Your changes to config.toml were saved.',
        color: 'green',
      })
      onSaved?.()
      onClose()

      return null
    } catch (error) {
      const message = errorMessage(error, 'Check the file for errors and try again.')

      if (mobileShell) {
        notifications.show({ title: "Couldn't save configuration", message, color: 'red' })
      }

      return message
    }
  }

  const [state, formAction, isPending] = useActionState(async (): Promise<EditorActionState> => {
    const submitted = content
    let result: ConfigAnalysisResponse | null = null

    if (validateConfigToml(submitted) === null) {
      try {
        result = await analyzeNow(submitted)
      } catch (error) {
        return {
          forContent: submitted,
          saveError: null,
          analysisError: errorMessage(error, 'Check the TOML syntax and try again.'),
        }
      }
    }

    if (!result?.valid) {
      if (mobileShell) {
        notifications.show({
          title: 'Configuration is invalid',
          message: 'Fix the validation errors before saving',
          color: 'red',
        })
      }

      return IDLE_STATE
    }

    if (result.migration_plan) {
      setSubmission({ kind: 'migration', content: submitted, plan: result.migration_plan, analysis: result })

      return IDLE_STATE
    }

    if (mobileShell && result.warnings.length > 0) {
      setSubmission({ kind: 'warnings' })
      openConfirm({
        zIndex: 400,
        title: 'Configuration warnings',
        children: (
          <Stack gap="xs">
            {result.warnings.map((warning) => (
              <Text key={`${warning.field}:${warning.message}`} size="sm">
                • {warning.message}
              </Text>
            ))}
          </Stack>
        ),
        labels: { confirm: 'Save anyway', cancel: 'Cancel' },
        confirmProps: { color: 'orange' },
        onConfirm: () => {
          void persist(submitted)
        },
        onClose: () => setSubmission(null),
      })

      return IDLE_STATE
    }

    const failure = await persist(submitted)

    return { forContent: submitted, saveError: mobileShell ? null : failure, analysisError: null }
  }, IDLE_STATE)

  const confirmMigration = async (options: MigrationOptions): Promise<void> => {
    if (submission?.kind !== 'migration') {
      return
    }

    try {
      const result = await migrateAndSave.mutateAsync({
        plan: submission.plan,
        options,
        content: submission.content,
      })

      notifications.show({
        title: 'Migration complete',
        message: `Migrated ${formatCount(result.stats.providers_migrated, 'provider')}`,
        color: 'green',
      })
      setSubmission(null)
      onSaved?.()
      onClose()
    } catch (error) {
      notifications.show({
        title: "Couldn't save configuration",
        message: errorMessage(error, 'Check the file for errors and try again.'),
        color: 'red',
      })

      throw error
    }
  }

  const submitting = isPending || submission !== null || saveConfig.isPending || migrateAndSave.isPending
  const saveLoading = submitting || background.isFetching
  const saveDisabled = validationError !== null || !hasChanges
  const current = state.forContent === content ? state : IDLE_STATE
  const backgroundError = background.error
    ? errorMessage(background.error, 'Check the TOML syntax and try again.')
    : null

  const footerContent = (requestClose: () => void): ReactNode => (
    <Group justify="space-between" w="100%">
      <Text size="xs" c="dimmed">
        {hasChanges ? 'Unsaved changes' : 'No changes'}
      </Text>
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={requestClose} disabled={submitting}>
          Cancel
        </Button>
        <Button size="sm" type="submit" form={formId} loading={saveLoading} disabled={saveDisabled}>
          Save
        </Button>
      </Group>
    </Group>
  )

  return (
    <StandardModal
      opened
      onClose={onClose}
      title="Edit config.toml"
      footer={footerContent}
      primaryAction={{
        label: 'Save',
        onClick: () => formRef.current?.requestSubmit(),
        loading: saveLoading,
        disabled: saveDisabled,
      }}
      dirty={hasChanges}
      loading={submitting}
      size="xl"
      contentPadding={false}
      disableScrollArea
    >
      <form id={formId} ref={formRef} action={formAction} style={FORM_STYLE}>
        <ConfigAnalysisNotices
          mobileShell={mobileShell}
          analyzing={background.isFetching}
          analysis={analysis}
          validationError={validationError}
          saveError={current.saveError}
          analysisError={current.analysisError ?? backgroundError}
        />
        <ConfigEditorField value={content} disabled={submitting} onChange={setContent} />
      </form>

      {submission?.kind === 'migration' && (
        <MigrationConfirmModal
          key={submission.content}
          onClose={() => setSubmission(null)}
          migrationPlan={submission.plan}
          stats={submission.analysis.stats}
          onConfirm={confirmMigration}
        />
      )}
    </StandardModal>
  )
}
