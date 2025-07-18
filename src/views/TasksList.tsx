import { useEffect, useState, useRef } from 'react'
import { Link, useNavigate } from 'react-router'
import { invoke } from '@tauri-apps/api/core'
import { useKeybinds } from '@/lib/use-keybinds'
import { Button } from '@/components/ui/button'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { toast } from 'sonner'

interface Task {
  id: string
  created_at: string
  task_description: string
}

export function TasksList() {
  const navigate = useNavigate()
  const [tasks, setTasks] = useState<Task[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const tableRef = useRef<HTMLTableElement>(null)

  // Load tasks on mount
  useEffect(() => {
    loadTasks()
  }, [])

  const loadTasks = async () => {
    try {
      setLoading(true)
      const result = await invoke<Task[]>('get_tasks')
      setTasks(result)
    } catch (error) {
      console.error('Error loading tasks:', error)
      toast(`Error loading tasks: ${error}`, {
        icon: '❌',
      })
    } finally {
      setLoading(false)
    }
  }

  const handleBack = () => navigate('/')
  const handleNewTask = () => navigate('/tasks/new')

  // Keyboard navigation
  useKeybinds([
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Backspace' && (event.metaKey || event.ctrlKey),
      callback: handleBack,
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowDown' || event.key === 'j',
      callback: () => {
        if (tasks.length > 0) {
          setSelectedIndex((prev) => Math.min(prev + 1, tasks.length - 1))
        }
      },
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'ArrowUp' || event.key === 'k',
      callback: () => {
        if (tasks.length > 0) {
          setSelectedIndex((prev) => Math.max(prev - 1, 0))
        }
      },
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'Enter' && (event.metaKey || event.ctrlKey),
      callback: handleNewTask,
    },
    {
      keyMatcher: (event: KeyboardEvent) => event.key === 'r' && (event.metaKey || event.ctrlKey),
      callback: loadTasks,
    },
  ])

  const truncateTask = (task: string, maxLength: number = 80) => {
    if (task.length <= maxLength) return task
    return task.substring(0, maxLength) + '...'
  }

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-foreground mx-auto mb-4"></div>
          <p className="text-muted-foreground">Loading tasks...</p>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-background p-4">
      <div className="max-w-6xl mx-auto">
        <div className="flex items-start justify-between mb-6">
          <h1 className="text-3xl font-bold">Tasks</h1>
          <div className="flex flex-col gap-2">
            <Link
              to="/"
              onClick={handleBack}
              className="text-sm text-muted-foreground hover:text-foreground flex justify-between items-center"
            >
              Back
              <span className="ml-2 text-xs text-muted-foreground">⌘⌫</span>
            </Link>
            <Link
              to="/tasks/new"
              onClick={handleNewTask}
              className="text-sm text-muted-foreground hover:text-foreground flex justify-between items-center"
            >
              New Task
              <span className="ml-2 text-xs text-muted-foreground">⌘↵</span>
            </Link>
          </div>
        </div>

        {tasks.length === 0 ? (
          <div className="text-center py-12">
            <p className="text-muted-foreground mb-4">No tasks yet.</p>
            <Button onClick={handleNewTask}>Create your first task</Button>
          </div>
        ) : (
          <div className="border rounded-lg">
            <Table ref={tableRef}>
              <TableHeader>
                <TableRow>
                  <TableHead>Task</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tasks.map((task, index) => (
                  <TableRow
                    key={task.id}
                    className={`cursor-pointer transition-colors ${index === selectedIndex ? 'bg-muted' : 'hover:bg-muted/50'}`}
                    onClick={() => setSelectedIndex(index)}
                  >
                    <TableCell className="font-mono text-sm">{truncateTask(task.task_description)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}

        <div className="mt-6 text-xs text-muted-foreground space-y-1">
          <p>• Press ↑/↓ or J/K to navigate</p>
          <p>• Press Cmd+Enter to create new task</p>
          <p>• Press Cmd+R to refresh</p>
          <p>• Press Cmd+Backspace to go back</p>
        </div>
      </div>
    </div>
  )
}
