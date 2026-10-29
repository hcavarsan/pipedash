import {
  Alert,
  Badge,
  Box,
  Button,
  Group,
  Loader,
  px,
  Stack,
  Text,
  useComputedColorScheme,
  useMantineTheme,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle, IconCheck, IconX } from '@tabler/icons-react'
import { lazy, type ReactNode, Suspense, useCallback, useEffect, useState } from 'react'
import { parse } from 'smol-toml'

import { useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { service } from '../../services'
import { SPACING_PX } from '../../theme'
import type { ConfigAnalysisResponse, MigrationOptions } from '../../types'
import { openConfirm } from '../../utils/confirm'
import { errorMessage } from '../../utils/errorDisplay'
import { formatCount } from '../../utils/formatCount'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'

import { loadTomlEditor } from './loadTomlEditor'
import { MigrationConfirmModal } from './MigrationConfirmModal'
import { TomlTextArea } from './TomlTextArea'

const TomlEditor = lazy(loadTomlEditor)

interface ConfigEditorModalProps {
  opened: boolean
  onClose: () => void
  initialContent: string
  onSaved?: () => void
}

export const ConfigEditorModal = ({ opened, onClose, initialContent, onSaved }: ConfigEditorModalProps): ReactNode => {
  const colorScheme = useComputedColorScheme('dark')
  const theme = useMantineTheme()
  const mobileShell = useMobileShell()
  const [content, setContent] = useState('')
  const [loading, setLoading] = useState(false)
  const [validationError, setValidationError] = useState<string | null>(null)
  const [hasChanges, setHasChanges] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [analysisError, setAnalysisError] = useState<string | null>(null)

  const [analysis, setAnalysis] = useState<ConfigAnalysisResponse | null>(null)
  const [analyzing, setAnalyzing] = useState(false)
  const [showMigrationModal, setShowMigrationModal] = useState(false)

  useEffect(() => {
    if (opened) {
      setContent(initialContent)
      setValidationError(null)
      setHasChanges(false)
      setSaveError(null)
      setAnalysisError(null)
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
      setAnalysisError(null)
      try {
        const result = await service.analyzeConfig(newContent)

        setAnalysis(result)

        const [firstError] = result.errors

        if (!result.valid && firstError) {
          setValidationError(firstError.message)
        } else if (validationError && result.valid) {
          setValidationError(null)
        }

        return result
      } catch (err) {
        logger.error('ConfigEditorModal', 'Analysis failed', err)
        setAnalysisError(errorMessage(err, 'Check the TOML syntax and try again.'))

        return null
      } finally {
        setAnalyzing(false)
      }
    },
    [validationError]
  )

  useEffect(() => {
    const timer = setTimeout(() => {
      if (content && content !== initialContent && !validationError) {
        void analyzeChanges(content)
      }
    }, 1000)

    return () => clearTimeout(timer)
  }, [content, initialContent, validationError, analyzeChanges])

  const handleEditorChange = (value: string | undefined) => {
    const newValue = value || ''

    setContent(newValue)
    setHasChanges(newValue !== initialContent)
    validateTOML(newValue)
    setSaveError(null)
    setAnalysis(null)
  }

  const handleSave = async () => {
    setSaveError(null)

    if (!validateTOML(content)) {
      if (mobileShell) {
        notifications.show({
          title: 'Configuration is invalid',
          message: 'Fix the validation errors before saving',
          color: 'red',
        })
      }
      return
    }

    const currentAnalysis = await analyzeChanges(content)

    if (!currentAnalysis?.valid) {
      if (mobileShell && currentAnalysis && !currentAnalysis.valid) {
        notifications.show({
          title: 'Configuration is invalid',
          message: 'Fix the validation errors before saving',
          color: 'red',
        })
      }
      return
    }

    if (currentAnalysis.migration_plan) {
      setShowMigrationModal(true)

      return
    }

    if (mobileShell && currentAnalysis.warnings.length > 0) {
      openConfirm({
        zIndex: 400,
        title: 'Configuration warnings',
        children: (
          <Stack gap="xs">
            {currentAnalysis.warnings.map((w) => (
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
      if (mobileShell) {
        notifications.show({
          title: "Couldn't save configuration",
          message: errorMessage(err, 'Check the file for errors and try again.'),
          color: 'red',
        })
      } else {
        setSaveError(errorMessage(err, 'Check the file for errors and try again.'))
      }
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
        title: "Couldn't save configuration",
        message: errorMessage(err, 'Check the file for errors and try again.'),
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const footerContent = (requestClose: () => void) => (
    <Group justify="space-between" w="100%">
      <Text size="xs" c="dimmed">
        {hasChanges ? 'Unsaved changes' : 'No changes'}
      </Text>
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={requestClose} disabled={loading}>
          Cancel
        </Button>
        <Button
          size="sm"
          onClick={handleSave}
          loading={loading || analyzing}
          disabled={!!validationError || !hasChanges}
        >
          Save
        </Button>
      </Group>
    </Group>
  )

  const noticesProps = mobileShell ? ({ mah: '35dvh', flex: '0 0 auto', style: { overflowY: 'auto' } } as const) : {}

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Edit config.toml"
      footer={footerContent}
      primaryAction={{
        label: 'Save',
        onClick: handleSave,
        loading: loading || analyzing,
        disabled: !!validationError || !hasChanges,
      }}
      dirty={hasChanges}
      loading={loading}
      size="xl"
      contentPadding={false}
      disableScrollArea
    >
      <Stack gap={0} style={{ flex: 1, overflow: 'hidden' }}>
        <Box {...noticesProps}>
          {analyzing &&
            (mobileShell ? (
              <Alert icon={<Loader size={16} />} color="gray" mx="lg" mt="md">
                <Text size="sm">Analyzing configuration...</Text>
              </Alert>
            ) : (
              <Box mx="lg" mt="md">
                <LoadingState variant="compact" message="Analyzing configuration…" />
              </Box>
            ))}

          {(saveError || analysisError) && (
            <Box mx="lg" mt="md">
              <FormErrorDisplay title="Couldn't save configuration" globalError={saveError} />
              <FormErrorDisplay title="Couldn't analyze configuration" globalError={analysisError} />
            </Box>
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

        {mobileShell ? (
          <TomlTextArea value={content} onChange={handleEditorChange} disabled={loading} />
        ) : (
          <Suspense fallback={<LoadingState message="Loading configuration editor…" />}>
            <TomlEditor
              height="100%"
              theme={colorScheme === 'dark' ? 'vs-dark' : 'light'}
              value={content}
              onChange={handleEditorChange}
              options={{
                fontSize: Number(px(theme.fontSizes.sm)),
                ariaLabel: 'Configuration editor',
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
                padding: { top: SPACING_PX.sm, bottom: SPACING_PX.sm },
              }}
            />
          </Suspense>
        )}
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
