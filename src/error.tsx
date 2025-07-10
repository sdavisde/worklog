import { useRouteError } from 'react-router'

export function ErrorBoundary() {
  const routeError = useRouteError() as any
  console.error(routeError)
  return (
    <div className='w-screen h-screen flex flex-col items-center justify-center'>
      <h1 className='text-2xl font-bold'>An unexpected error occurred</h1>
      <p className='text-muted-foreground'>Please restart Worklog</p>

      {'error' in routeError && (
        <p className='text-sm text-muted-foreground mt-4'>Error: {(routeError.error as Error).message}</p>
      )}
    </div>
  )
}
