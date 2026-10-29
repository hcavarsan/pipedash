import { Box } from '@mantine/core'
import { DataTable, type DataTableProps } from 'mantine-datatable'

const INTERACTIVE_TARGET = 'button, a, input, label, [role="checkbox"]'

export function StandardTable<T>(props: DataTableProps<T>) {
  return (
    <Box
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
        flex: 1,
        position: 'relative',
      }}
    >
      <DataTable<T>
        {...props}
        withTableBorder={false}
        rowClassName={props.onRowClick ? 'interactive-surface row-clickable' : 'interactive-surface'}
        {...(props.onRowClick
          ? {
              onRowClick: (params) => {
                const target = params.event.target

                if (
                  target instanceof Element &&
                  params.event.currentTarget.contains(target) &&
                  !target.closest(INTERACTIVE_TARGET)
                ) {
                  props.onRowClick?.(params)
                }
              },
            }
          : {})}
        withColumnBorders={false}
        scrollAreaProps={{
          type: 'auto',
          scrollbarSize: 10,
          scrollHideDelay: 0,
        }}
        styles={{
          ...props.styles,
          header: {
            ...(props.styles?.header || {}),
            fontSize: 'var(--mantine-font-size-sm)',
            fontWeight: 600,
            paddingTop: 'var(--mantine-spacing-sm)',
            paddingBottom: 'var(--mantine-spacing-sm)',
            paddingLeft: 'var(--mantine-spacing-md)',
            paddingRight: 'var(--mantine-spacing-md)',
            whiteSpace: 'nowrap',
            verticalAlign: 'middle',
            borderBottom: '1px solid var(--mantine-color-default-border)',
          },
          table: {
            ...(props.styles?.table || {}),
            borderSpacing: '0',
            tableLayout: 'auto',
            width: '100%',
            fontSize: 'var(--mantine-font-size-sm)',
            fontVariantNumeric: 'tabular-nums',
          },
          pagination: {
            ...(props.styles?.pagination || {}),
            paddingTop: 'var(--mantine-spacing-xs)',
            fontSize: 'var(--mantine-font-size-sm)',
          },
        }}
        rowStyle={(record, index) => [{ height: '48px', verticalAlign: 'middle' }, props.rowStyle?.(record, index)]}
        height={props.height || '100%'}
        verticalSpacing="xs"
        horizontalSpacing="md"
      />
    </Box>
  )
}
