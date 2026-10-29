import type { ReactNode } from 'react'
import { ErrorState } from '../../../../components/common/ErrorState'
import { LoadingState } from '../../../../components/common/LoadingState'
import { useDefaultDataDir, useStorageConfig } from '../../../../queries/useStorageQueries'
import { useVaultPasswordStatus } from '../../../../queries/useVaultQueries'

import { SetupWizardLayout } from './SetupWizardLayout'
import { SetupWizardSession } from './SetupWizardSession'
import type { SetupWizardProps } from './types'

export const SetupWizard = ({ opened, onComplete, onClose }: SetupWizardProps): ReactNode => {
  const config = useStorageConfig({ enabled: opened })
  const vault = useVaultPasswordStatus({ enabled: opened })
  const defaultDir = useDefaultDataDir({ enabled: opened })

  if (!opened) {
    return null
  }

  if (config.data !== undefined && vault.data !== undefined && defaultDir.data !== undefined) {
    return (
      <SetupWizardSession
        opened={opened}
        currentConfig={config.data}
        vaultStatus={vault.data}
        defaultDataDir={defaultDir.data}
        onComplete={onComplete}
        onClose={onClose}
      />
    )
  }

  const failed = [config, vault, defaultDir].find((query) => query.isError)
  const retrying = config.isFetching || vault.isFetching || defaultDir.isFetching

  return (
    <SetupWizardLayout
      opened={opened}
      settings={Boolean(onClose)}
      closable={Boolean(onClose)}
      onClose={onClose ?? onComplete}
      title={onClose ? 'Storage settings' : 'Welcome to Pipedash'}
      onboardingTitle="Welcome to Pipedash"
      busy={false}
      stepper={null}
      region={null}
      form={null}
      footer={null}
      shellFooter={null}
      primaryAction={undefined}
      onBack={undefined}
    >
      {failed ? (
        <ErrorState
          title="Couldn't load storage configuration"
          error={failed.error}
          onRetry={() => {
            for (const query of [config, vault, defaultDir]) {
              if (query.isError) {
                void query.refetch()
              }
            }
          }}
          retrying={retrying}
        />
      ) : (
        <LoadingState message="Loading storage configuration…" />
      )}
    </SetupWizardLayout>
  )
}
