import { Tabs as TabsPrimitive } from '@base-ui/react/tabs'
import { cn } from 'cn'

function Tabs({ className, ...props }: TabsPrimitive.Root.Props) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      className={cn('tabs-root flex flex-col', className)}
      {...props}
    />
  )
}

function TabsList({ className, ...props }: TabsPrimitive.List.Props) {
  return (
    <TabsPrimitive.List
      data-slot="tabs-list"
      className={cn(
        'tabs-list inline-flex w-fit items-center gap-0.5 rounded-md border border-border bg-muted p-0.5 text-muted-foreground',
        className,
      )}
      {...props}
    />
  )
}

function TabsTab({ className, ...props }: TabsPrimitive.Tab.Props) {
  return (
    <TabsPrimitive.Tab
      data-slot="tabs-tab"
      className={cn(
        'tabs-tab inline-flex h-6 shrink-0 items-center justify-center gap-1.5 rounded-sm px-2.5 text-xs font-medium whitespace-nowrap text-muted-foreground transition-colors outline-none select-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:opacity-50 aria-selected:bg-background aria-selected:text-foreground aria-selected:shadow-2xs',
        className,
      )}
      {...props}
    />
  )
}

function TabsPanel({ className, ...props }: TabsPrimitive.Panel.Props) {
  return (
    <TabsPrimitive.Panel
      data-slot="tabs-panel"
      className={cn('tabs-panel min-h-0 flex-1 outline-none', className)}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsPanel, TabsTab }
