import { Box } from '@mantine/core'
import { DataTable, type DataTableProps } from 'mantine-datatable'

export function StandardTable<T>(props: DataTableProps<T>) {
  return (
    <Box
      mt="xl"
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
      }}
    >
      <DataTable<T>
        {...props}
        withTableBorder={false}
        highlightOnHover
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
            fontSize: 'var(--mantine-font-size-md)',
            fontWeight: 600,
            paddingTop: '1rem',
            paddingBottom: '1rem',
            paddingLeft: '1rem',
            paddingRight: '1rem',
            whiteSpace: 'nowrap',
            verticalAlign: 'middle',
            textAlign: 'left',
            borderBottom: '1px solid var(--mantine-color-default-border)',
          },
          table: {
            ...(props.styles?.table || {}),
            borderSpacing: '0',
            tableLayout: 'auto',
            width: '100%',
            fontSize: 'var(--mantine-font-size-sm)',
          },
          pagination: {
            ...(props.styles?.pagination || {}),
            paddingTop: '1rem',
            fontSize: 'var(--mantine-font-size-sm)',
          },
        }}
        rowStyle={(record, index) => [{ height: '56px', verticalAlign: 'middle' }, props.rowStyle?.(record, index)]}
        height={props.height || '100%'}
        verticalSpacing="sm"
        horizontalSpacing="md"
      />
    </Box>
  )
}
