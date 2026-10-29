import { Button, FileButton, Group, Loader } from '@mantine/core'
import { IconDownload, IconUpload } from '@tabler/icons-react'
import { type ReactNode, useRef } from 'react'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useExportProviders, useImportProviders, useProviders } from '../../../queries/useProvidersQueries'
import { LiveStatus } from '../../forms/LiveStatus'
import { IosGroup, IosRow } from '../../ios/IosList'

export const ProviderTransferActions = (): ReactNode => {
  const mobileShell = useMobileShell()
  const { data: providers = [], isLoading: loading } = useProviders()
  const importMutation = useImportProviders()
  const exportMutation = useExportProviders()
  const resetRef = useRef<() => void>(null)

  const importing = importMutation.isPending
  const exporting = exportMutation.isPending
  const status = importing ? 'Importing providers' : exporting ? 'Exporting providers' : ''

  const handleImport = (file: File | null) => {
    if (file) {
      importMutation.mutate(file)
      resetRef.current?.()
    }
  }

  if (mobileShell) {
    return (
      <>
        <LiveStatus message={status} />
        <IosGroup>
          <FileButton accept=".toml" resetRef={resetRef} onChange={handleImport}>
            {(props) => (
              <IosRow
                title="Import providers"
                disabled={loading || importing}
                onClick={props.onClick}
                accessory={importing ? <Loader size={18} aria-hidden /> : <IconUpload size={18} aria-hidden />}
              />
            )}
          </FileButton>
          <IosRow
            title="Export providers"
            disabled={loading || providers.length === 0 || exporting}
            onClick={() => exportMutation.mutate()}
            accessory={exporting ? <Loader size={18} aria-hidden /> : <IconDownload size={18} aria-hidden />}
          />
        </IosGroup>
      </>
    )
  }

  return (
    <Group gap="xs">
      <LiveStatus message={status} />
      <FileButton accept=".toml" resetRef={resetRef} onChange={handleImport}>
        {(props) => (
          <Button
            {...props}
            size="compact-sm"
            variant="light"
            color="gray"
            leftSection={<IconUpload size={14} aria-hidden />}
            loading={importing}
            disabled={loading}
          >
            Import
          </Button>
        )}
      </FileButton>
      <Button
        size="compact-sm"
        variant="light"
        color="gray"
        leftSection={<IconDownload size={14} aria-hidden />}
        onClick={() => exportMutation.mutate()}
        loading={exporting}
        disabled={loading || providers.length === 0}
      >
        Export
      </Button>
    </Group>
  )
}
