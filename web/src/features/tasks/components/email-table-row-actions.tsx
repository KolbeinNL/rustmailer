/*
 * Copyright © 2025-2026 rustmailer.com
 * Licensed under RustMailer License Agreement v1.0
 * Unauthorized use or distribution is prohibited.
 */

import { DotsHorizontalIcon } from '@radix-ui/react-icons'
import { Row } from '@tanstack/react-table'
import { IconRefresh, IconTrash } from '@tabler/icons-react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useTasksContext } from '../context'
import { EmailTask, TaskStatus } from '../data/schema'
import { MessageSquareMore } from 'lucide-react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from '@/hooks/use-toast'
import { retry_email_task } from '@/api/tasks/api'

interface EmailTableRowActionsProps {
  row: Row<EmailTask>
}

export function EmailTableRowActions({ row }: EmailTableRowActionsProps) {
  const { setOpen, setCurrentEmailRow } = useTasksContext()
  const queryClient = useQueryClient()

  const retryMutation = useMutation({
    mutationFn: () => retry_email_task(row.original.id),
    retry: 0,
    onSuccess: () => {
      toast({
        title: 'Task requeued',
        description: `Task ${row.original.id} will be sent again.`,
      })
      queryClient.invalidateQueries({ queryKey: ['send-email-tasks'] })
    },
    onError: (error: Error) => {
      toast({
        title: 'Failed to retry task',
        description: `${error.message}`,
        variant: 'destructive',
      })
    },
  })

  const retryable =
    row.original.status === TaskStatus.Failed ||
    row.original.status === TaskStatus.Stopped

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            variant='ghost'
            className='flex h-8 w-8 p-0 data-[state=open]:bg-muted'
          >
            <DotsHorizontalIcon className='h-4 w-4' />
            <span className='sr-only'>Open menu</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-[160px]'>
          <DropdownMenuItem
            onClick={() => {
              setCurrentEmailRow(row.original)
              setOpen('detail')
            }}
          >
            Detail
            <DropdownMenuShortcut>
              <MessageSquareMore size={16} />
            </DropdownMenuShortcut>
          </DropdownMenuItem>
          {retryable && (
            <DropdownMenuItem
              disabled={retryMutation.isPending}
              onClick={() => retryMutation.mutate()}
            >
              Retry
              <DropdownMenuShortcut>
                <IconRefresh size={16} />
              </DropdownMenuShortcut>
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={() => {
              setCurrentEmailRow(row.original)
              setOpen('delete')
            }}
            className='!text-red-500'
          >
            Remove
            <DropdownMenuShortcut>
              <IconTrash size={16} />
            </DropdownMenuShortcut>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  )
}
