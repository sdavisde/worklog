import { useRef, useEffect } from 'react'
import { useNavigate } from 'react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from '@/components/ui/form'
import { useKeybinds } from '@/lib/use-keybinds'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'

const taskSchema = z.object({
  task: z.string().min(1, 'Task is required').max(500, 'Task must be less than 500 characters'),
})

type TaskFormData = z.infer<typeof taskSchema>

export function TaskForm() {
  const navigate = useNavigate()
  const taskFieldRef = useRef<HTMLTextAreaElement>(null)

  const form = useForm<TaskFormData>({
    resolver: zodResolver(taskSchema),
    defaultValues: {
      task: '',
    },
  })

  // Auto-focus task input on mount
  useEffect(() => {
    console.log('setting focus on ', taskFieldRef.current)
    setTimeout(() => taskFieldRef.current?.focus(), 500)
  }, [taskFieldRef.current])

  const onSubmit = async (data: TaskFormData) => {
    try {
      const result = await invoke<string>('save_task', { task: data.task })
      toast(`Added task: ${data.task}`, {
        icon: '✅',
      })

      // Reset form and stay on page for demo
      form.reset()
      form.setFocus('task')
      // navigate('/')
    } catch (error) {
      console.error('Error saving task:', error)
      toast(`Error saving task: ${error}`, {
        icon: '❌',
      })
    }
  }

  const handleCancel = () => navigate('/')

  // Keyboard shortcuts
  useKeybinds([
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Backspace' && (event.metaKey || event.ctrlKey),
      callback: handleCancel,
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Enter' && (event.metaKey || event.ctrlKey),
      callback: () => {
        if (form.formState.isValid) {
          form.handleSubmit(onSubmit)()
        }
      },
    },
  ])

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <div className="w-full max-w-md">
        <h1 className="text-2xl font-bold mb-6 text-center">Add New Task</h1>

        <Form {...form}>
          <form
            onSubmit={form.handleSubmit(onSubmit)}
            className="space-y-4"
          >
            <FormField
              control={form.control}
              name="task"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>
                    Task <span className="text-red-500">*</span>
                  </FormLabel>
                  <FormControl>
                    <Textarea
                      {...field}
                      ref={taskFieldRef}
                      placeholder="Enter your task..."
                      rows={4}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <div className="flex gap-2 pt-4">
              <Button
                type="button"
                variant="outline"
                onClick={handleCancel}
                className="flex-1"
              >
                Cancel
                <span className="ml-2 text-xs text-gray-500">⌘ Backspace</span>
              </Button>
              <Button
                type="submit"
                className="flex-1"
                disabled={!form.formState.isValid}
              >
                Add Task
                <span className="ml-2 text-xs text-gray-500">⌘↵</span>
              </Button>
            </div>
          </form>
        </Form>
      </div>
    </div>
  )
}
