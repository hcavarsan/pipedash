import { Alert, Badge, Box, Group, Loader, Text } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { ConfigAnalysisResponse } from '../../types'
import { formatCount } from '../../utils/formatCount'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { LoadingState } from '../common/LoadingState'
import { LiveStatus } from '../forms/LiveStatus'

interface ConfigAnalysisNoticesProps {
  mobileShell: boolean
  analyzing: boolean
  analysis: ConfigAnalysisResponse | null
  validationError: string | null
  saveError: string | null
  analysisError: string | null
}

const ANALYZING_MESSAGE = 'Analyzing configuration…'

export function ConfigAnalysisNotices({
  mobileShell,
  analyzing,
  analysis,
  validationError,
  saveError,
  analysisError,
}: ConfigAnalysisNoticesProps): ReactNode {
  const noticesProps = mobileShell ? ({ mah: '35dvh', flex: '0 0 auto', style: { overflowY: 'auto' } } as const) : {}
  const { stats, migration_plan: migrationPlan, postgres_connection: postgres } = analysis ?? {}

  return (
    <Box {...noticesProps}>
      {mobileShell && <LiveStatus message={analyzing ? ANALYZING_MESSAGE : ''} />}
      {analyzing &&
        (mobileShell ? (
          <Alert icon={<Loader size={16} />} color="gray" mx="lg" mt="md" role="presentation" aria-hidden>
            <Text size="sm">{ANALYZING_MESSAGE}</Text>
          </Alert>
        ) : (
          <Box mx="lg" mt="md">
            <LoadingState variant="compact" message={ANALYZING_MESSAGE} />
          </Box>
        ))}

      {(saveError || analysisError) && (
        <Box mx="lg" mt="md">
          <FormErrorDisplay title="Couldn't save configuration" globalError={saveError} />
          <FormErrorDisplay title="Couldn't analyze configuration" globalError={analysisError} />
        </Box>
      )}

      {migrationPlan && (
        <Alert
          icon={<IconAlertCircle size={16} aria-hidden />}
          color="orange"
          title="Migration required"
          mx="lg"
          mt="md"
        >
          <Text size="sm" fw={500}>
            These changes require migrating {migrationPlan.steps.length} components to the new storage backend.
          </Text>
          {stats && mobileShell && (
            <Text size="xs" c="dimmed" mt="xs">
              {[
                formatCount(stats.providers_count, 'provider'),
                formatCount(stats.tokens_count, 'token'),
                formatCount(stats.cache_entries_count, 'cache entry', 'cache entries'),
              ].join(' · ')}
            </Text>
          )}
          {stats && !mobileShell && (
            <Group gap="xs" mt="xs">
              <Badge size="sm">{formatCount(stats.providers_count, 'provider')}</Badge>
              <Badge size="sm">{formatCount(stats.tokens_count, 'token')}</Badge>
              <Badge size="sm">{formatCount(stats.cache_entries_count, 'cache entry', 'cache entries')}</Badge>
            </Group>
          )}
        </Alert>
      )}

      {analysis?.errors.map((error) => (
        <Alert
          key={`${error.field}:${error.message}`}
          icon={<IconAlertCircle size={16} aria-hidden />}
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
          icon={<IconAlertCircle size={16} aria-hidden />}
          color="orange"
          title={warning.field}
          mx="lg"
          mt="md"
        >
          <Text size="sm">{warning.message}</Text>
        </Alert>
      ))}

      {postgres && (
        <Alert
          icon={postgres.success ? <IconCheck size={16} aria-hidden /> : <IconX size={16} aria-hidden />}
          color={postgres.success ? 'green' : 'red'}
          title={postgres.success ? 'PostgreSQL connection successful' : 'PostgreSQL connection failed'}
          mx="lg"
          mt="md"
        >
          <Text size="sm">{postgres.message}</Text>
          {postgres.latency_ms && (
            <Text size="xs" c="dimmed" mt={4}>
              Connection latency: {postgres.latency_ms}ms
            </Text>
          )}
        </Alert>
      )}

      {validationError && !analysis && (
        <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="red" title="Validation error" mx="lg" mt="md">
          <Text size="sm">{validationError}</Text>
        </Alert>
      )}
    </Box>
  )
}
