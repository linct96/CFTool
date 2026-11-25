import { Dialog as DialogPrimitive } from '@base-ui/react/dialog'
import { cn } from 'cn'
import { XIcon } from 'lucide-react'
import type * as React from 'react'
import { Button } from '@/components/ui/button'

function Sheet({ ...props }: DialogPrimitive.Root.Props) {
  return <DialogPrimitive.Root data-slot="sheet" {...props} />
}

function SheetTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
  return <DialogPrimitive.Trigger data-slot="sheet-trigger" {...props} />
}

function SheetClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot="sheet-close" {...props} />
}

function SheetOverlay({ className, ...props }: DialogPrimitive.Backdrop.Props) {
  return (
    <DialogPrimitive.Backdrop
      data-slot="sheet-overlay"
      className={cn(
        'sheet-overlay fixed inset-0 isolate z-50 bg-overlay duration-150 supports-backdrop-filter:backdrop-blur-[1px] data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0',
        className,
      )}
      {...props}
    />
  )
}

function SheetContent({
  className,
  children,
  keepMounted,
  showCloseButton = true,
  ...props
}: DialogPrimitive.Popup.Props & {
  keepMounted?: DialogPrimitive.Portal.Props['keepMounted']
  showCloseButton?: boolean
}) {
  return (
    <DialogPrimitive.Portal keepMounted={keepMounted}>
      <SheetOverlay />
      <DialogPrimitive.Popup
        data-slot="sheet-content"
        className={cn(
          'sheet-content fixed inset-y-0 right-0 z-50 flex h-dvh w-full max-w-[min(26rem,90vw)] flex-col border-l border-border bg-background text-xs/relaxed text-foreground shadow-sheet outline-none transition-[max-width] duration-200 data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0',
          className,
        )}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close
            data-slot="sheet-close"
            render={
              <Button
                variant="ghost"
                className="sheet-close absolute top-2.5 right-2.5"
                size="icon-sm"
              />
            }
          >
            <XIcon aria-hidden="true" />
            <span className="sr-only">关闭</span>
          </DialogPrimitive.Close>
        )}
      </DialogPrimitive.Popup>
    </DialogPrimitive.Portal>
  )
}

function SheetHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="sheet-header"
      className={cn(
        'sheet-header flex h-11 shrink-0 items-center gap-2 border-b border-border px-4 pr-11',
        className,
      )}
      {...props}
    />
  )
}

function SheetTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      data-slot="sheet-title"
      className={cn('sheet-title truncate text-sm font-semibold tracking-tight', className)}
      {...props}
    />
  )
}

function SheetFooter({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="sheet-footer"
      className={cn(
        'sheet-footer flex shrink-0 items-center justify-end gap-2 border-t border-border bg-sidebar px-4 py-2.5',
        className,
      )}
      {...props}
    />
  )
}

export { Sheet, SheetClose, SheetContent, SheetFooter, SheetHeader, SheetTitle, SheetTrigger }
