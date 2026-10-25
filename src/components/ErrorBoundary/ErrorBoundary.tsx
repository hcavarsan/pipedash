import { Button, Center, Code, Group, Paper, Stack, Text, ThemeIcon, Title } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import { Component, type ErrorInfo, type ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'

import { StatusScreen } from './StatusScreen'

type Scope = 'app' | 'route' | 'feature'

interface ErrorBoundaryProps {
  children: ReactNode
  scope?: Scope
  name?: string
}

interface ErrorBoundaryState {
  error: Error | null
  componentStack: string | null
}

const SCOPE_MIN_HEIGHT: Record<Scope, string | number> = {
  app: '100vh',
  route: 400,
  feature: 200,
}

const SCOPE_TITLE: Record<Scope, string> = {
  app: 'Pipedash hit an unexpected error',
  route: 'This page failed to load',
  feature: 'This section failed to load',
}

interface BoundaryFallbackProps {
  scope: Scope
  name?: string
  error: Error
  componentStack: string | null
  onRetry: () => void
}

function BoundaryFallback({ scope, name, error, componentStack, onRetry }: BoundaryFallbackProps) {
  const mobileShell = useMobileShell()
  const title = name ? `${name} failed to load` : SCOPE_TITLE[scope]

  if (mobileShell) {
    return (
      <StatusScreen
        icon={
          <ThemeIcon variant="transparent" color="red" size={48}>
            <IconAlertTriangle size={40} aria-hidden />
          </ThemeIcon>
        }
        title={title}
        message={error.message || 'An unexpected error occurred'}
        minHeight={scope === 'app' ? '100dvh' : 400}
        detail={
          import.meta.env.DEV && componentStack ? (
            <Code block fz="xs" mah={200} w="100%" style={{ overflow: 'auto' }}>
              {componentStack}
            </Code>
          ) : null
        }
        actions={
          <>
            <Button size="md" fullWidth onClick={() => window.location.reload()}>
              Reload page
            </Button>
            <Button size="md" fullWidth variant="default" onClick={onRetry}>
              Try again
            </Button>
          </>
        }
      />
    )
  }

  return (
    <Center mih={SCOPE_MIN_HEIGHT[scope]} p="xl" role="alert">
      <Paper withBorder p="xl" maw={500} w="100%">
        <Stack align="center" gap="md">
          <ThemeIcon variant="transparent" color="red" size="xl">
            <IconAlertTriangle size={24} aria-hidden />
          </ThemeIcon>
          <Stack align="center" gap="xs">
            <Title order={3}>{title}</Title>
            <Text size="sm" c="dimmed" ta="center">
              {error.message || 'An unexpected error occurred'}
            </Text>
          </Stack>
          {import.meta.env.DEV && componentStack && (
            <Code block fz="xs" mah={200} w="100%" style={{ overflow: 'auto' }}>
              {componentStack}
            </Code>
          )}
          <Group justify="center" gap="sm">
            <Button size="sm" onClick={() => window.location.reload()}>
              Reload page
            </Button>
            <Button size="sm" variant="default" onClick={onRetry}>
              Try again
            </Button>
          </Group>
        </Stack>
      </Paper>
    </Center>
  )
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null, componentStack: null }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    const { scope = 'feature', name } = this.props

    logger.error('ErrorBoundary', `${name ?? scope} caught an error`, {
      message: error.message,
      stack: error.stack,
      componentStack: errorInfo.componentStack,
    })
    this.setState({ componentStack: errorInfo.componentStack ?? null })
  }

  handleRetry = (): void => {
    this.setState({ error: null, componentStack: null })
  }

  render(): ReactNode {
    const { error, componentStack } = this.state

    if (error) {
      return (
        <BoundaryFallback
          scope={this.props.scope ?? 'feature'}
          name={this.props.name}
          error={error}
          componentStack={componentStack}
          onRetry={this.handleRetry}
        />
      )
    }

    return this.props.children
  }
}
