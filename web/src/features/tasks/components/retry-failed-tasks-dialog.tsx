/*
 * Copyright © 2025-2026 rustmailer.com
 * Licensed under RustMailer License Agreement v1.0
 * Unauthorized use or distribution is prohibited.
 */

import { IconRefresh } from '@tabler/icons-react'
import { toast } from '@/hooks/use-toast'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { retry_email_tasks } from '@/api/tasks/api'

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function RetryFailedTasksDialog({ open, onOpenChange }: Props) {
  const queryClient = useQueryClient()
  const retryMutation = useMutation({
    mutationFn: () => retry_email_tasks(),
    retry: 0,
    onSuccess: (result) => {
      toast({
        title: 'Failed tasks requeued',
        description:
          result.retried > 0
            ? `${result.retried} task${result.retried === 1 ? '' : 's'} will be sent again.`
            : 'There were no failed tasks to retry.',
      })
      queryClient.invalidateQueries({ queryKey: ['send-email-tasks'] })
      onOpenChange(false)
    },
    onError: (error: Error) => {
      toast({
        title: 'Failed to retry tasks',
        description: `${error.message}`,
        variant: 'destructive',
      })
    },
  })

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      handleConfirm={() => retryMutation.mutate()}
      isLoading={retryMutation.isPending}
      className='max-w-2xl'
      title={
        <span>
          <IconRefresh className='mr-1 inline-block' size={18} /> Retry All Failed Tasks
        </span>
      }
      desc={
        <div className='space-y-4'>
          <p className='mb-2'>
            Every email task currently in the{' '}
            <span className='font-bold'>Failed</span> state will be requeued and
            sent again. The tasks keep their original request data, so there is
            no need to resend the original requests.
          </p>
          <Alert>
            <AlertTitle>Heads up</AlertTitle>
            <AlertDescription>
              Retried emails are loaded from the local email cache. Tasks whose
              cached content has already been evicted will fail again.
            </AlertDescription>
          </Alert>
        </div>
      }
      confirmText='Retry'
    />
  )
}
