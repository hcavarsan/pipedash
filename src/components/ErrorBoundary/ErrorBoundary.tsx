import { Button, Center, Code, Stack } from '@mantine/core'
import { Component, type ErrorInfo, type ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { ErrorState } from '../common/ErrorState'

type Scope = 'app' | 'route' | 'feature'

interface ErrorBoundaryProps {
  children: ReactNode
  scope?: Scope
  name?: string | undefined
  resetKeys?: readonly unknown[] | undefined
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
  route: "Couldn't load this page",
  feature: "Couldn't load this section",
}

interface BoundaryFallbackProps {
  scope: Scope
  name?: string | undefined
  error: Error
  componentStack: string | null
  onRetry: () => void
}

function keysChanged(previous: readonly unknown[] | undefined, next: readonly unknown[] | undefined): boolean {
  if (previous === next) {
    return false
  }
  if (!previous || !next || previous.length !== next.length) {
    return true
  }

  return previous.some((value, index) => !Object.is(value, next[index]))
}

function BoundaryFallback({ scope, name, error, componentStack, onRetry }: BoundaryFallbackProps): ReactNode {
  const mobileShell = useMobileShell()

  return (
    <Center mih={SCOPE_MIN_HEIGHT[scope]} p={{ base: 'md', sm: 'xl' }}>
      <Stack maw={500} w="100%">
        <ErrorState
          title={name ? `Couldn't load ${name}` : SCOPE_TITLE[scope]}
          error={error.message || 'An unexpected error occurred'}
          onRetry={onRetry}
          actions={
            <Button
              size={mobileShell ? 'md' : 'sm'}
              variant="default"
              fullWidth={mobileShell}
              onClick={() => window.location.reload()}
            >
              Reload page
            </Button>
          }
        />
        {import.meta.env.DEV && componentStack && (
          <Code block fz="xs" mah={200} w="100%" style={{ overflow: 'auto' }}>
            {componentStack}
          </Code>
        )}
      </Stack>
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

  componentDidUpdate(previous: ErrorBoundaryProps, previousState: ErrorBoundaryState): void {
    if (previousState.error && this.state.error && keysChanged(previous.resetKeys, this.props.resetKeys)) {
      this.setState({ error: null, componentStack: null })
    }
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
