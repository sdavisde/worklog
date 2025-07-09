import './App.css'
import { Button } from '@/components/ui/button'

function App() {
  return (
    <div className='min-h-screen flex items-center justify-center bg-background'>
      <div className='text-center'>
        <h1 className='text-4xl font-bold text-foreground mb-4'>Hello world</h1>
        <p className='text-muted-foreground'>Welcome to Tauri + React</p>
        <Button>Click me</Button>
      </div>
    </div>
  )
}

export default App
