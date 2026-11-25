import { useRef } from 'react'
import type { ComponentProps } from 'react'
import { NumberField } from '@base-ui/react/number-field'
import { ChevronDownIcon, ChevronUpIcon } from 'lucide-react'
import { cn } from 'cn'
import { InputGroup, InputGroupInput } from './input-group'

const format: Intl.NumberFormatOptions = { useGrouping: false, maximumFractionDigits: 20 }
const stepperClassName =
  'flex min-h-0 w-full items-center justify-center text-muted-foreground transition-colors outline-none hover:not-disabled:bg-accent hover:not-disabled:text-foreground active:not-disabled:bg-accent-hover focus-visible:bg-accent aria-disabled:pointer-events-none aria-disabled:opacity-40 [&>svg]:size-3'

type InputNumberProps = Pick<
  ComponentProps<typeof NumberField.Root>,
  'id' | 'disabled' | 'value' | 'min' | 'max' | 'step' | 'onBlur'
> & {
  inputProps: Pick<
    ComponentProps<typeof NumberField.Input>,
    'placeholder' | 'aria-invalid' | 'aria-describedby'
  >
  onValueChange: (value: number | null) => void
}

export function InputNumber({ inputProps, onValueChange, step = 1, ...props }: InputNumberProps) {
  const smallStep = step === 'any' ? 1 : step
  const handledInput = useRef(false)
  const wasInputHandled = () => handledInput.current

  return (
    <NumberField.Root
      data-slot="input-number"
      className="min-w-0 tabular-nums"
      step={step}
      smallStep={smallStep}
      largeStep={smallStep * 10}
      format={format}
      allowOutOfRange
      onValueChange={(value) => {
        handledInput.current = true
        onValueChange(value)
      }}
      {...props}
    >
      <NumberField.Group
        render={<InputGroup />}
        className="overflow-hidden data-disabled:opacity-50"
      >
        <NumberField.Input
          aria-roledescription="数字输入"
          {...inputProps}
          render={(attributes) => (
            <InputGroupInput
              {...attributes}
              onChange={(event) => {
                handledInput.current = false
                attributes.onChange?.(event)
                // Base UI retains the last valid number when the user types a partial value.
                if (
                  !wasInputHandled() &&
                  !event.defaultPrevented &&
                  !event.nativeEvent.defaultPrevented
                ) {
                  onValueChange(event.currentTarget.value.trim() ? NaN : null)
                }
              }}
            />
          )}
        />
        <div className="grid h-full w-6 shrink-0 grid-rows-2 self-stretch border-s border-input">
          <NumberField.Increment className={stepperClassName} aria-label="增加">
            <ChevronUpIcon aria-hidden="true" />
          </NumberField.Increment>
          <NumberField.Decrement
            className={cn(stepperClassName, 'border-t border-input')}
            aria-label="减少"
          >
            <ChevronDownIcon aria-hidden="true" />
          </NumberField.Decrement>
        </div>
      </NumberField.Group>
    </NumberField.Root>
  )
}
