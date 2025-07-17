import { useRef, useEffect } from 'react'
import { useNavigate } from 'react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from '@/components/ui/form'
import { useKeybinds } from '@/lib/use-keybinds'
import { toast } from 'sonner'

const taskSchema = z.object({
  title: z.string().min(1, 'Title is required').max(100, 'Title must be less than 100 characters'),
  description: z.string().optional(),
})

type TaskFormData = z.infer<typeof taskSchema>

export function TaskFormRHF() {
  const navigate = useNavigate()
  const titleInputRef = useRef<HTMLInputElement>(null)
  const descriptionInputRef = useRef<HTMLTextAreaElement>(null)
  
  const form = useForm<TaskFormData>({
    resolver: zodResolver(taskSchema),
    defaultValues: {
      title: '',
      description: '',
    },
  })

  // Auto-focus title input on mount
  useEffect(() => {
    titleInputRef.current?.focus()
  }, [])

  const onSubmit = (data: TaskFormData) => {
    // TODO: Save task to storage
    toast(`Added task: ${data.title}`, { 
      description: data.description || undefined, 
      icon: '✅' 
    })
    
    // Reset form and stay on page for demo
    form.reset()
    titleInputRef.current?.focus()
    // navigate('/')
  }

  const handleCancel = () => {
    navigate('/')
  }

  // Keyboard shortcuts
  useKeybinds([
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Escape',
      callback: handleCancel,
    },
    {
      keyMatcher: (event: KeyboardEvent) => 
        event.key === 'Enter' && (event.metaKey || event.ctrlKey),
      callback: () => {
        if (form.formState.isValid) {
          form.handleSubmit(onSubmit)()
        }
      },
    },
  ])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Enter key in title field moves to description
    if (e.key === 'Enter' && e.target === titleInputRef.current) {
      e.preventDefault()
      descriptionInputRef.current?.focus()
    }

    // Enter in description submits form
    if (e.key === 'Enter' && e.target === descriptionInputRef.current) {
      e.preventDefault()
      if (form.formState.isValid) {
        form.handleSubmit(onSubmit)()
      }
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <div className="w-full max-w-md">
        <h1 className="text-2xl font-bold mb-6 text-center">Add New Task (RHF)</h1>
        
        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="title"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>
                    Title <span className="text-red-500">*</span>
                  </FormLabel>
                  <FormControl>
                    <Input
                      {...field}
                      ref={titleInputRef}
                      placeholder="Enter task title..."
                      onKeyDown={handleKeyDown}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Description</FormLabel>
                  <FormControl>
                    <Textarea
                      {...field}
                      ref={descriptionInputRef}
                      placeholder="Enter task description... (optional)"
                      rows={3}
                      onKeyDown={handleKeyDown}
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
                <span className="ml-2 text-xs text-gray-500">Esc</span>
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

        <div className="mt-6 text-xs text-gray-500 text-center space-y-1">
          <p>• Press Tab to navigate between fields</p>
          <p>• Press Enter in title to move to description</p>
          <p>• Press Enter in description to submit</p>
          <p>• Press Cmd+Enter to submit from anywhere</p>
          <p>• Press Escape to cancel</p>
        </div>
      </div>
    </div>
  )
}