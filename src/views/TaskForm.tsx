import { useEffect } from 'react'
import { useNavigate } from 'react-router'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { useKeybinds } from '@/lib/use-keybinds'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'

const addTaskSchema = z.object({
  task: z.string().min(1).max(500),
})

type AddTaskFormData = z.infer<typeof addTaskSchema>

export function TaskForm() {
  const navigate = useNavigate()
  const form = useForm<AddTaskFormData>({
    defaultValues: {
      task: '',
    },
    resolver: zodResolver(addTaskSchema),
  })

  // Auto-focus task input on mount
  useEffect(() => form.setFocus('task'), [])

  const handleSubmit = async (formData: AddTaskFormData) => {
    try {
      const result = await invoke<string>('save_task', { task: formData.task })
      toast(`Added task: ${formData.task}`, {
        icon: '✅',
        description: result,
      })

      // Reset form and stay on page
      form.setValue('task', '')
      form.setFocus('task')
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
      callback: () => form.handleSubmit(handleSubmit)(),
    },
  ])

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <div className="w-full max-w-md">
        <h1 className="text-2xl font-bold mb-6 text-center">Add New Task</h1>

        <form
          onSubmit={form.handleSubmit(handleSubmit)}
          className="space-y-4"
        >
          <div>
            <label
              htmlFor="task"
              className="block text-sm font-medium mb-2"
            >
              Task <span className="text-red-500">*</span>
            </label>
            <Textarea
              {...form.register('task')}
              placeholder="Enter your task..."
              rows={4}
              required
            />
          </div>

          {form.formState.errors.task && <p className="text-red-500 text-sm mt-1">{form.formState.errors.task.message}</p>}

          <div className="flex gap-2 pt-4">
            <Button
              type="button"
              variant="outline"
              onClick={handleCancel}
              className="flex-1"
            >
              Cancel
              <span className="ml-2 text-xs text-gray-500">⌘⌫</span>
            </Button>
            <Button
              type="submit"
              className="flex-1"
            >
              Add Task
              <span className="ml-2 text-xs text-gray-500">⌘↵</span>
            </Button>
          </div>
        </form>

        <div className="mt-6 text-xs text-gray-500 text-center space-y-1">
          <p>• Press Shift+Enter to submit</p>
          <p>• Press Cmd+Enter to submit from anywhere</p>
          <p>• Press Cmd+Backspace to cancel</p>
        </div>
      </div>
    </div>
  )
}
