import { Button, Group, Text } from '@mantine/core'
import { IconRefresh } from '@tabler/icons-react'
import type { DataTableColumn } from 'mantine-datatable'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/domain'

import {
  buildColumnItems,
  columnPreferencesOf,
  columnSignature,
  moveColumn,
  resetColumnItems,
} from '../lib/columnCustomization'
import { DesktopColumnCustomization } from './DesktopColumnCustomization'
import { IosColumnCustomization } from './IosColumnCustomization'

export interface TableCustomizationSessionProps {
  opened: boolean
  onClose: () => void
  columns: DataTableColumn<PipelineRun>[]
  visibleColumns: DataTableColumn<PipelineRun>[]
  onApply: (columnOrder: string[], columnVisibility: Record<string, boolean>) => Promise<void>
  currentOrder?: string[] | undefined
  currentVisibility?: Record<string, boolean> | undefined
}

interface ApplyState {
  error: string | null
}

export function TableCustomizationSession({
  opened,
  onClose,
  columns,
  visibleColumns,
  onApply,
  currentOrder,
  currentVisibility,
}: TableCustomizationSessionProps): ReactNode {
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const [items, setItems] = useState(() => buildColumnItems(columns, visibleColumns, currentOrder, currentVisibility))
  const [initialSignature] = useState(() => columnSignature(items))
  const [search, setSearch] = useState('')

  const [state, formAction, isPending] = useActionState(
    async (): Promise<ApplyState> => {
      const { order, visibility } = columnPreferencesOf(items)

      try {
        await onApply(order, visibility)
      } catch (error) {
        return { error: error instanceof Error ? error.message : "Couldn't save the column preferences." }
      }

      onClose()

      return { error: null }
    },
    { error: null }
  )

  const body = {
    items,
    search,
    onSearchChange: setSearch,
    onToggle: (id: string) =>
      setItems((previous) => previous.map((item) => (item.id === id ? { ...item, visible: !item.visible } : item))),
    onMove: (id: string, direction: -1 | 1) => setItems((previous) => moveColumn(previous, id, direction)),
    onShowAll: () => setItems((previous) => previous.map((item) => ({ ...item, visible: true }))),
    onHideOptional: () =>
      setItems((previous) => previous.map((item) => (item.locked ? item : { ...item, visible: false }))),
    onReset: () => setItems(resetColumnItems(columns)),
  }

  const dirty = items.length > 0 && columnSignature(items) !== initialSignature
  const visibleCount = items.filter((item) => item.visible).length

  const form = (
    <form
      id={formId}
      ref={formRef}
      action={formAction}
      style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}
    >
      <FormErrorDisplay title="Couldn't save columns" globalError={state.error} />
      {mobileShell ? <IosColumnCustomization {...body} /> : <DesktopColumnCustomization {...body} />}
    </form>
  )

  if (mobileShell) {
    return (
      <StandardModal
        opened={opened}
        onClose={onClose}
        title="Customize columns"
        size="md"
        fullHeight
        dirty={dirty}
        primaryAction={{
          label: 'Apply',
          loading: isPending,
          onClick: () => formRef.current?.requestSubmit(),
        }}
      >
        {form}
      </StandardModal>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={
        <Group gap="xs">
          <Text fw={TYPOGRAPHY.MODAL_TITLE.weight} size={TYPOGRAPHY.MODAL_TITLE.size}>
            Customize columns
          </Text>
          <Text size={TYPOGRAPHY.HELPER_TEXT.size} c="dimmed">
            {visibleCount}/{items.length}
          </Text>
        </Group>
      }
      footer={(close) => (
        <Group justify="space-between" gap="sm" wrap="nowrap">
          <Button
            type="button"
            variant="subtle"
            color="gray"
            size="sm"
            leftSection={<IconRefresh size={16} aria-hidden />}
            onClick={body.onReset}
          >
            Reset
          </Button>
          <Group gap="sm" wrap="nowrap">
            <Button type="button" variant="default" size="sm" onClick={close}>
              Cancel
            </Button>
            <Button type="submit" form={formId} size="sm" loading={isPending}>
              Apply
            </Button>
          </Group>
        </Group>
      )}
      dirty={dirty}
      size="lg"
      disableScrollArea
      contentPadding={false}
    >
      {form}
    </StandardModal>
  )
}
