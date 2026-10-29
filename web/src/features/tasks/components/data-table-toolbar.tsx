/*
 * Copyright © 2025-2026 rustmailer.com
 * Licensed under RustMailer License Agreement v1.0
 * Unauthorized use or distribution is prohibited.
 */

import { Table } from '@tanstack/react-table'
import { IconRefresh } from '@tabler/icons-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { TaskStatus } from '../data/schema'

interface DataTableToolbarProps<TData> {
  table: Table<TData>
  placeHolder: string
  statusFilter?: TaskStatus | 'all'
  onStatusFilterChange: (status: TaskStatus | 'all') => void
  onRetryAllFailed?: () => void
}

const statusOptions = [
  { value: 'all', label: 'All' },
  { value: TaskStatus.Scheduled, label: 'Scheduled' },
  { value: TaskStatus.Running, label: 'Running' },
  { value: TaskStatus.Success, label: 'Success' },
  { value: TaskStatus.Failed, label: 'Failed' },
  { value: TaskStatus.Removed, label: 'Removed' },
  { value: TaskStatus.Stopped, label: 'Stopped' }
] as const

export function DataTableToolbar<TData>({
  table,
  placeHolder,
  statusFilter = 'all',
  onStatusFilterChange,
  onRetryAllFailed
}: DataTableToolbarProps<TData>) {
  return (
    <div className="flex items-center gap-4 flex-wrap sm:flex-nowrap">
      <div className="flex-shrink-0 w-full sm:w-64">
        <Input
          placeholder={placeHolder}
          value={(table.getState().globalFilter as string) ?? ''}
          onChange={(event) => {
            table.setGlobalFilter(event.target.value)
          }}
          className="h-8 w-full"
        />
      </div>
      <div className="flex flex-wrap gap-4">
        {statusOptions.map((option) => (
          <Button
            key={option.value}
            variant={statusFilter === option.value ? 'default' : 'outline'}
            size="sm"
            className="text-xs px-3 py-1 h-8 whitespace-nowrap"
            onClick={() => onStatusFilterChange(option.value)}
          >
            {option.label}
          </Button>
        ))}
      </div>
      {onRetryAllFailed && (
        <Button
          variant="outline"
          size="sm"
          className="ml-auto h-8 whitespace-nowrap text-xs"
          onClick={onRetryAllFailed}
        >
          <IconRefresh size={16} className="mr-1" />
          Retry all failed
        </Button>
      )}
    </div>
  )
}