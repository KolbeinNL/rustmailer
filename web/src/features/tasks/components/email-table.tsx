/*
 * Copyright © 2025-2026 rustmailer.com
 * Licensed under RustMailer License Agreement v1.0
 * Unauthorized use or distribution is prohibited.
 */

import { useState } from 'react'
import {
  ColumnDef,
  ColumnFiltersState,
  RowData,
  SortingState,
  VisibilityState,
  flexRender,
  getCoreRowModel,
  getFacetedRowModel,
  getFacetedUniqueValues,
  getSortedRowModel,
  getFilteredRowModel,
  useReactTable,
} from '@tanstack/react-table'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { EmailTask, TaskStatus } from '../data/schema'
import { DataTablePagination } from './data-table-pagination'
import { DataTableToolbar } from './data-table-toolbar'
import { RetryFailedTasksDialog } from './retry-failed-tasks-dialog'
import { useQuery } from '@tanstack/react-query'
import Logo from '@/assets/logo.svg'
import { TableSkeleton } from '@/components/table-skeleton'
import { list_email_tasks } from '@/api/tasks/api'

declare module '@tanstack/react-table' {
  interface ColumnMeta<TData extends RowData, TValue> {
    className: string
  }
}

interface DataTableProps {
  columns: ColumnDef<EmailTask>[],
  onCallback?: () => void;
}

export function EmailTaskTable({ columns }: DataTableProps) {
  const [rowSelection, setRowSelection] = useState({})
  const [columnVisibility, setColumnVisibility] = useState<VisibilityState>({})
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([])
  const [sorting, setSorting] = useState<SortingState>([])
  const [pagination, setPagination] = useState({
    pageIndex: 0,
    pageSize: 10,
  });
  const [statusFilter, setStatusFilter] = useState<TaskStatus | 'all'>('all');
  const [retryDialogOpen, setRetryDialogOpen] = useState(false);

  const { data: queryResult, isLoading } = useQuery({
    queryKey: ['send-email-tasks', pagination.pageIndex, pagination.pageSize, statusFilter],
    queryFn: () => list_email_tasks(pagination.pageIndex, pagination.pageSize, statusFilter),
  });

  const total = queryResult?.total_items || 0;

  const table = useReactTable({
    data: queryResult?.items || [],
    columns,
    state: {
      sorting,
      columnVisibility,
      rowSelection,
      columnFilters,
      pagination
    },
    enableRowSelection: true,
    onRowSelectionChange: setRowSelection,
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onColumnVisibilityChange: setColumnVisibility,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    onPaginationChange: setPagination,
    manualPagination: true,
    rowCount: total,
    pageCount: queryResult?.total_pages || 0,
    getSortedRowModel: getSortedRowModel(),
    getFacetedRowModel: getFacetedRowModel(),
    getFacetedUniqueValues: getFacetedUniqueValues(),
    globalFilterFn: (row, _, filterValue) => {
      const searchValue = filterValue.toLowerCase();
      const checkValue = (value: any): boolean => {
        if (typeof value === 'string') {
          return value.toLowerCase().includes(searchValue);
        }
        if (typeof value === 'object' && value !== null) {
          return Object.values(value).some((nestedValue) =>
            checkValue(nestedValue)
          );
        }
        return false;
      };

      return checkValue(row.original);
    }
  });

  const handleStatusFilterChange = (status: TaskStatus | 'all') => {
    setStatusFilter(status);
    setPagination((prev) => ({ ...prev, pageIndex: 0 }));
  };

  return (
    <div className="space-y-4">
      {/* Always render DataTableToolbar */}
      <DataTableToolbar
        table={table}
        placeHolder="Filter email tasks..."
        statusFilter={statusFilter}
        onStatusFilterChange={handleStatusFilterChange}
        onRetryAllFailed={() => setRetryDialogOpen(true)}
      />
      <RetryFailedTasksDialog
        open={retryDialogOpen}
        onOpenChange={setRetryDialogOpen}
      />
      {isLoading ? (
        <TableSkeleton columns={columns.length} rows={10} />
      ) : total > 0 ? (
        <div>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id} className="group/row">
                    {headerGroup.headers.map((header) => (
                      <TableHead
                        key={header.id}
                        colSpan={header.colSpan}
                        className={header.column.columnDef.meta?.className ?? ''}
                      >
                        {header.isPlaceholder
                          ? null
                          : flexRender(
                            header.column.columnDef.header,
                            header.getContext()
                          )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {table.getRowModel().rows?.length ? (
                  table.getRowModel().rows.map((row) => (
                    <TableRow
                      key={row.id}
                      data-state={row.getIsSelected() && 'selected'}
                      className="group/row"
                    >
                      {row.getVisibleCells().map((cell) => (
                        <TableCell
                          key={cell.id}
                          className={cell.column.columnDef.meta?.className ?? ''}
                        >
                          {flexRender(
                            cell.column.columnDef.cell,
                            cell.getContext()
                          )}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell
                      colSpan={columns.length}
                      className="h-24 text-center"
                    >
                      No results.
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </div>
          <DataTablePagination table={table} />
        </div>
      ) : (
        <div className="flex h-[450px] shrink-0 items-center justify-center rounded-md border border-dashed">
          <div className="mx-auto flex max-w-[420px] flex-col items-center justify-center text-center">
            <img
              src={Logo}
              className="max-h-[100px] w-auto opacity-20 saturate-0 transition-all duration-300 hover:opacity-100 hover:saturate-100 object-contain"
              alt="RustMailer Logo"
            />
            <h3 className="mt-4 text-lg font-semibold">
              {statusFilter === 'all' ? 'No Email Tasks Available' : `No Email Tasks for ${statusFilter} Status`}
            </h3>
            <p className="mb-4 mt-2 text-sm text-muted-foreground">
              {statusFilter === 'all'
                ? 'The email outbox is currently empty. Create a new email task via the API to get started.'
                : `No email tasks match the ${statusFilter} status. Try a different status or create a new email task via the API.`}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}