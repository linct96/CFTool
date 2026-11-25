import type { ComponentProps } from 'react'
import { cn } from 'cn'
import { Input } from './input'

function InputGroup({ className, ...props }: ComponentProps<'fieldset'>) {
  return (
    <fieldset
      data-slot="input-group"
      className={cn(
        'flex h-7 w-full min-w-0 items-center rounded-md border border-input bg-input/20 transition-colors has-[[data-slot=input-group-control]:focus-visible]:border-ring has-[[data-slot=input-group-control]:focus-visible]:ring-2 has-[[data-slot=input-group-control]:focus-visible]:ring-ring/30 has-[[data-slot=input-group-control][aria-invalid=true]]:border-destructive has-[[data-slot=input-group-control][aria-invalid=true]]:ring-2 has-[[data-slot=input-group-control][aria-invalid=true]]:ring-destructive/20 dark:bg-input/30 dark:has-[[data-slot=input-group-control][aria-invalid=true]]:ring-destructive/40',
        className,
      )}
      {...props}
    />
  )
}

function InputGroupInput({ className, ...props }: ComponentProps<'input'>) {
  return (
    <Input
      data-slot="input-group-control"
      className={cn(
        'flex-1 rounded-none border-0 bg-transparent shadow-none ring-0 focus-visible:ring-0 aria-invalid:ring-0 disabled:opacity-100 dark:bg-transparent',
        className,
      )}
      {...props}
    />
  )
}

export { InputGroup, InputGroupInput }
