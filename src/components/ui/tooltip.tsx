import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'

const TooltipProvider = TooltipPrimitive.Provider
const Tooltip = TooltipPrimitive.Root
const TooltipTrigger = TooltipPrimitive.Trigger

function TooltipContent({
  children,
  side = 'top',
}: {
  children: React.ReactNode
  side?: TooltipPrimitive.Positioner.Props['side']
}) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Positioner side={side} sideOffset={4} className="isolate z-50">
        <TooltipPrimitive.Popup className="rounded-md bg-foreground px-3 py-1.5 text-xs text-background">
          {children}
        </TooltipPrimitive.Popup>
      </TooltipPrimitive.Positioner>
    </TooltipPrimitive.Portal>
  )
}

export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger }
