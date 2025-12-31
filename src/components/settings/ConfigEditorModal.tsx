import { useCallback, useEffect, useState } from 'react'

import { parse } from '@iarna/toml'
import { Alert, Badge, Button, Group, Loader, Stack, Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import Editor from '@monaco-editor/react'
import { IconAlertCircle, IconCheck, IconX } from '@tabler/icons-react'

import { service } from '../../services'
import type { ConfigAnalysisResponse, MigrationOptions } from '../../types'
import { StandardModal } from '../common/StandardModal'

import { MigrationConfirmModal } from './MigrationConfirmModal'

interface ConfigEditorModalProps {
  opened: boolean
  onClose: () => void
  initialContent: string
  onSaved?: () => void
}

export const ConfigEditorModal = ({
  opened,
  onClose,
  initialContent,
  onSaved,
}: ConfigEditorModalProps) => {
  const [content, setContent] = useState('')
  const [loading, setLoading] = useState(false)
  const [validationError, setValidationError] = useState<string | null>(null)
  const [hasChanges, setHasChanges] = useState(false)

  const [analysis, setAnalysis] = useState<ConfigAnalysisResponse | null>(null)
  const [analyzing, setAnalyzing] = useState(false)
  const [showMigrationModal, setShowMigrationModal] = useState(false)

  useEffect(() => {
    if (opened) {
      setContent(initialContent)
      setValidationError(null)
      setHasChanges(false)
      setAnalysis(null)
      setAnalyzing(false)
    }
  }, [opened, initialContent])

  const validateTOML = (toml: string): boolean => {
    if (!toml.trim()) {
      setValidationError('Config file cannot be empty')

return false
    }

    try {
      parse(toml)
      setValidationError(null)

return true
    } catch (err) {
      const error = err instanceof Error ? err.message : 'Invalid TOML syntax'

      setValidationError(error)

return false
    }
  }

  const analyzeChanges = useCallback(async (newContent: string) => {
    if (!newContent.trim()) {
      return
    }

    setAnalyzing(true)
    try {
      const result = await service.analyzeConfig(newContent)

      setAnalysis(result)

      if (!result.valid && result.errors.length > 0) {
        setValidationError(result.errors[0].message)
      } else if (validationError && result.valid) {
        setValidationError(null)
      }
    } catch (err) {
      console.error('Analysis failed:', err)
    } finally {
      setAnalyzing(false)
    }
  }, [validationError])

  useEffect(() => {
    const timer = setTimeout(() => {
      if (content && content !== initialContent && !validationError) {
        analyzeChanges(content)
      }
    }, 1000)

    return () => clearTimeout(timer)
  }, [content, initialContent, validationError, analyzeChanges])

  const handleEditorChange = (value: string | undefined) => {
    const newValue = value || ''

    setContent(newValue)
    setHasChanges(newValue !== initialContent)
    validateTOML(newValue)
  }

  const handleSave = async () => {
    if (!validationError) {
      await analyzeChanges(content)
    }

    if (!validateTOML(content)) {
      notifications.show({
        title: 'Validation Error',
        message: 'Please fix the validation errors before saving',
        color: 'red',
      })

return
    }

    if (analysis && !analysis.valid) {
      notifications.show({
        title: 'Validation Error',
        message: 'Please fix validation errors before saving',
        color: 'red',
      })

return
    }

    if (analysis?.migration_plan) {
      setShowMigrationModal(true)

return
    }

    if (analysis?.warnings && analysis.warnings.length > 0) {
      modals.openConfirmModal({
        title: 'Configuration Warnings',
        children: (
          <Stack gap="xs">
            {analysis.warnings.map((w, i) => (
              <Text key={i} size="sm">
                • {w.message}
              </Text>
            ))}
          </Stack>
        ),
        labels: { confirm: 'Save Anyway', cancel: 'Cancel' },
        confirmProps: { color: 'orange' },
        onConfirm: () => saveWithoutMigration(),
      })

return
    }

    await saveWithoutMigration()
  }

  const saveWithoutMigration = async () => {
    setLoading(true)
    try {
      await service.saveConfigContent(content)
      notifications.show({
        title: 'Success',
        message: 'Configuration saved successfully',
        color: 'green',
      })
      onSaved?.()
      onClose()
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to save configuration'

      notifications.show({
        title: 'Error',
        message: errorMessage,
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const handleMigrationConfirm = async (options: MigrationOptions) => {
    if (!analysis?.migration_plan) {
      return
    }

    setLoading(true)
    try {
      const result = await service.executeMigration(analysis.migration_plan, options)

      if (!result.success) {
        throw new Error(result.errors?.join(', ') || 'Migration failed')
      }

      await service.saveConfigContent(content)

      notifications.show({
        title: 'Migration Complete',
        message: `Migrated ${result.stats.providers_migrated} providers successfully`,
        color: 'green',
      })

      setShowMigrationModal(false)
      onSaved?.()
      onClose()
    } catch (err) {
      notifications.show({
        title: 'Migration Failed',
        message: err instanceof Error ? err.message : 'Unknown error',
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const handleClose = () => {
    if (hasChanges) {
      modals.openConfirmModal({
        title: 'Unsaved Changes',
        centered: true,
        zIndex: 400,
        children: (
          <Text size="sm">
            You have unsaved changes. Are you sure you want to close without saving?
          </Text>
        ),
        labels: { confirm: 'Close', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: onClose,
      })

return
    }
    onClose()
  }

  const footerContent = (
    <Group justify="space-between" w="100%">
      <Text size="xs" c="dimmed">
        {hasChanges ? 'Unsaved changes' : 'No changes'}
      </Text>
      <Group gap="xs">
        <Button variant="subtle" onClick={handleClose} disabled={loading}>
          Cancel
        </Button>
        <Button
          onClick={handleSave}
          loading={loading}
          disabled={!!validationError || !hasChanges}
        >
          Save
        </Button>
      </Group>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={handleClose}
      title="Edit config.toml"
      footer={footerContent}
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, overflow: 'hidden' }}>
        {analyzing && (
          <Alert
            icon={<Loader size={16} />}
            color="blue"
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm">Analyzing configuration...</Text>
          </Alert>
        )}

        {analysis?.migration_plan && (
          <Alert
            icon={<IconAlertCircle size={16} />}
            color="orange"
            title="Migration Required"
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm" fw={500}>
              These changes require migrating {analysis.migration_plan.steps.length} components to the new
              storage backend.
            </Text>
            {analysis.stats && (
              <Group gap="xs" mt="xs">
                <Badge size="sm">{analysis.stats.providers_count} providers</Badge>
                <Badge size="sm">{analysis.stats.tokens_count} tokens</Badge>
                <Badge size="sm">{analysis.stats.cache_entries_count} cache entries</Badge>
              </Group>
            )}
          </Alert>
        )}

        {analysis?.errors.map((error, i) => (
          <Alert
            key={i}
            icon={<IconAlertCircle size={16} />}
            color="red"
            title={error.field}
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm">{error.message}</Text>
            <Text size="xs" c="dimmed" mt={4}>
              Error code: {error.code}
            </Text>
          </Alert>
        ))}

        {analysis?.warnings.map((warning, i) => (
          <Alert
            key={i}
            icon={<IconAlertCircle size={16} />}
            color="yellow"
            title={warning.field}
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm">{warning.message}</Text>
          </Alert>
        ))}

        {analysis?.postgres_connection && (
          <Alert
            icon={analysis.postgres_connection.success ? <IconCheck size={16} /> : <IconX size={16} />}
            color={analysis.postgres_connection.success ? 'green' : 'red'}
            title={analysis.postgres_connection.success ? 'PostgreSQL Connection Successful' : 'PostgreSQL Connection Failed'}
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm">{analysis.postgres_connection.message}</Text>
            {analysis.postgres_connection.latency_ms && (
              <Text size="xs" c="dimmed" mt={4}>
                Connection latency: {analysis.postgres_connection.latency_ms}ms
              </Text>
            )}
          </Alert>
        )}

        {validationError && !analysis && (
          <Alert
            icon={<IconAlertCircle size={16} />}
            color="red"
            title="Validation Error"
            styles={{
              root: {
                margin: 'var(--mantine-spacing-md)',
                marginBottom: 0,
              },
            }}
          >
            <Text size="sm">{validationError}</Text>
          </Alert>
        )}

        <Editor
          height="100%"
          language="ini"
          theme="vs-dark"
          value={content}
          onChange={handleEditorChange}
          options={{
            fontSize: 14,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap: 'on',
            automaticLayout: true,
            tabSize: 2,
            insertSpaces: true,
            lineNumbers: 'on',
            glyphMargin: false,
            folding: true,
            lineDecorationsWidth: 0,
            lineNumbersMinChars: 3,
            padding: { top: 12, bottom: 12 },
          }}
        />
      </Stack>

      {analysis?.migration_plan && (
        <MigrationConfirmModal
          opened={showMigrationModal}
          onClose={() => setShowMigrationModal(false)}
          migrationPlan={analysis.migration_plan}
          stats={analysis.stats}
          onConfirm={handleMigrationConfirm}
        />
      )}
    </StandardModal>
  )
}
