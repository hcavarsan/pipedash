import { Alert, Badge, Box, Button, Group, Loader, Stack, Text, useComputedColorScheme } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle, IconCheck, IconX } from '@tabler/icons-react'
import { lazy, Suspense, useCallback, useEffect, useState } from 'react'
import { parse } from 'smol-toml'

import { useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { service } from '../../services'
import type { ConfigAnalysisResponse, MigrationOptions } from '../../types'
import { formatErrorMessage } from '../../types/errors'
import { openConfirm } from '../../utils/confirm'
import { formatCount } from '../../utils/formatCount'
import { StandardModal } from '../common/StandardModal'

import { MigrationConfirmModal } from './MigrationConfirmModal'

const TomlEditor = lazy(() => import('./TomlEditor'))

interface ConfigEditorModalProps {
  opened: boolean
  onClose: () => void
  initialContent: string
  onSaved?: () => void
}

export const ConfigEditorModal = ({ opened, onClose, initialContent, onSaved }: ConfigEditorModalProps) => {
  const colorScheme = useComputedColorScheme('dark')
  const mobileShell = useMobileShell()
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

  const analyzeChanges = useCallback(
    async (newContent: string) => {
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
        logger.error('ConfigEditorModal', 'Analysis failed', err)
      } finally {
        setAnalyzing(false)
      }
    },
    [validationError]
  )

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
        title: 'Configuration is invalid',
        message: 'Fix the validation errors before saving',
        color: 'red',
      })

      return
    }

    if (analysis && !analysis.valid) {
      notifications.show({
        title: 'Configuration is invalid',
        message: 'Fix the validation errors before saving',
        color: 'red',
      })

      return
    }

    if (analysis?.migration_plan) {
      setShowMigrationModal(true)

      return
    }

    if (analysis?.warnings && analysis.warnings.length > 0) {
      openConfirm({
        zIndex: mobileShell ? 400 : undefined,
        title: 'Configuration warnings',
        children: (
          <Stack gap="xs">
            {analysis.warnings.map((w) => (
              <Text key={`${w.field}:${w.message}`} size="sm">
                • {w.message}
              </Text>
            ))}
          </Stack>
        ),
        labels: { confirm: 'Save anyway', cancel: 'Cancel' },
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
        title: 'Configuration saved',
        message: 'Your changes to config.toml were saved.',
        color: 'green',
      })
      onSaved?.()
      onClose()
    } catch (err) {
      const errorMessage = formatErrorMessage(err)

      notifications.show({
        title: "Couldn't save configuration",
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
        title: 'Migration complete',
        message: `Migrated ${formatCount(result.stats.providers_migrated, 'provider')}`,
        color: 'green',
      })

      setShowMigrationModal(false)
      onSaved?.()
      onClose()
    } catch (err) {
      notifications.show({
        title: 'Migration failed',
        message: formatErrorMessage(err),
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const handleClose = () => {
    if (hasChanges) {
      openConfirm({
        title: 'Discard changes?',
        centered: true,
        zIndex: 400,
        children: <Text size="sm">Your edits to config.toml have not been saved and will be lost.</Text>,
        labels: { confirm: 'Discard changes', cancel: 'Keep editing' },
        confirmProps: { color: 'red' },
        onConfirm: onClose,
      })

      return
    }
    onClose()
  }

  const footerContent = mobileShell ? (
    <Stack gap="xs">
      <Text size="xs" c="dimmed" ta="center">
        {hasChanges ? 'Unsaved changes' : 'No changes'}
      </Text>
      <Button fullWidth onClick={handleSave} loading={loading} disabled={!!validationError || !hasChanges}>
        Save
      </Button>
    </Stack>
  ) : (
    <Group justify="space-between" w="100%">
      <Text size="xs" c="dimmed">
        {hasChanges ? 'Unsaved changes' : 'No changes'}
      </Text>
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={handleClose} disabled={loading}>
          Cancel
        </Button>
        <Button size="sm" onClick={handleSave} loading={loading} disabled={!!validationError || !hasChanges}>
          Save
        </Button>
      </Group>
    </Group>
  )

  const noticesProps = mobileShell ? ({ mah: '35dvh', flex: '0 0 auto', style: { overflowY: 'auto' } } as const) : {}

  return (
    <StandardModal
      opened={opened}
      onClose={handleClose}
      title="Edit config.toml"
      footer={footerContent}
      size="xl"
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, overflow: 'hidden' }}>
        <Box {...noticesProps}>
          {analyzing && (
            <Alert icon={<Loader size={16} />} color="gray" mx="lg" mt="md">
              <Text size="sm">Analyzing configuration...</Text>
            </Alert>
          )}

          {analysis?.migration_plan && (
            <Alert icon={<IconAlertCircle size={16} />} color="orange" title="Migration required" mx="lg" mt="md">
              <Text size="sm" fw={500}>
                These changes require migrating {analysis.migration_plan.steps.length} components to the new storage
                backend.
              </Text>
              {analysis.stats && mobileShell && (
                <Text size="xs" c="dimmed" mt="xs">
                  {[
                    formatCount(analysis.stats.providers_count, 'provider'),
                    formatCount(analysis.stats.tokens_count, 'token'),
                    formatCount(analysis.stats.cache_entries_count, 'cache entry', 'cache entries'),
                  ].join(' · ')}
                </Text>
              )}
              {analysis.stats && !mobileShell && (
                <Group gap="xs" mt="xs">
                  <Badge size="sm">{formatCount(analysis.stats.providers_count, 'provider')}</Badge>
                  <Badge size="sm">{formatCount(analysis.stats.tokens_count, 'token')}</Badge>
                  <Badge size="sm">
                    {formatCount(analysis.stats.cache_entries_count, 'cache entry', 'cache entries')}
                  </Badge>
                </Group>
              )}
            </Alert>
          )}

          {analysis?.errors.map((error) => (
            <Alert
              key={`${error.field}:${error.message}`}
              icon={<IconAlertCircle size={16} />}
              color="red"
              title={error.field}
              mx="lg"
              mt="md"
            >
              <Text size="sm">{error.message}</Text>
              <Text size="xs" c="dimmed" mt={4}>
                Error code: {error.code}
              </Text>
            </Alert>
          ))}

          {analysis?.warnings.map((warning) => (
            <Alert
              key={`${warning.field}:${warning.message}`}
              icon={<IconAlertCircle size={16} />}
              color="orange"
              title={warning.field}
              mx="lg"
              mt="md"
            >
              <Text size="sm">{warning.message}</Text>
            </Alert>
          ))}

          {analysis?.postgres_connection && (
            <Alert
              icon={analysis.postgres_connection.success ? <IconCheck size={16} /> : <IconX size={16} />}
              color={analysis.postgres_connection.success ? 'green' : 'red'}
              title={
                analysis.postgres_connection.success
                  ? 'PostgreSQL connection successful'
                  : 'PostgreSQL connection failed'
              }
              mx="lg"
              mt="md"
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
            <Alert icon={<IconAlertCircle size={16} />} color="red" title="Validation error" mx="lg" mt="md">
              <Text size="sm">{validationError}</Text>
            </Alert>
          )}
        </Box>

        <Suspense fallback={<Loader size="sm" m="lg" />}>
          <TomlEditor
            height="100%"
            theme={colorScheme === 'dark' ? 'vs-dark' : 'light'}
            value={content}
            onChange={handleEditorChange}
            options={{
              fontSize: mobileShell ? 16 : 14,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              automaticLayout: true,
              tabSize: 2,
              insertSpaces: true,
              lineNumbers: mobileShell ? 'off' : 'on',
              glyphMargin: false,
              folding: true,
              lineDecorationsWidth: 0,
              lineNumbersMinChars: 3,
              padding: { top: 12, bottom: 12 },
            }}
          />
        </Suspense>
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
