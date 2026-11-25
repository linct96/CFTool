import { useStore } from '@tanstack/react-form'
import type { ReactNode } from 'react'
import { defaultCfstUrl, isCfstSettingEnabled, numberSettings } from '@/cfst-settings-schema'
import type { NumberSetting } from '@/cfst-settings-schema'
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { InputNumber } from '@/components/ui/input-number'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { CfstSettingsForm } from '@/use-cfst-settings-form'

const probeModes = [
  { label: 'TCPing', value: 'tcp' },
  { label: 'HTTPing', value: 'http' },
] as const

const settingField = 'cfst-setting-field min-w-0 gap-1.5'
const settingsCard = 'cfst-settings-card rounded-lg border border-border bg-card p-4'
const settingsGridTwo = 'cfst-settings-group grid min-w-0 gap-3 @min-[24rem]:grid-cols-2'
const settingsGridThree =
  'cfst-settings-group grid min-w-0 gap-3 @min-[24rem]:grid-cols-2 @min-[34rem]:grid-cols-3'

function SettingField({
  label,
  flag,
  disabled,
  meta,
  children,
}: {
  label: string
  flag: string
  disabled: boolean
  meta: { isTouched: boolean; isValid: boolean; errors: ({ message: string } | undefined)[] }
  children: (inputProps: {
    id: string
    'aria-invalid': boolean
    'aria-describedby'?: string
  }) => ReactNode
}) {
  const id = `cfst-${flag}`
  const invalid = !disabled && meta.isTouched && !meta.isValid
  const errorId = `${id}-error`

  return (
    <Field className={settingField} data-disabled={disabled} data-invalid={invalid}>
      <FieldLabel htmlFor={id} className="cfst-setting-label min-w-0">
        <span className="cfst-setting-label-text">
          {label} <code className="cfst-setting-flag font-mono text-muted-foreground">-{flag}</code>
        </span>
      </FieldLabel>
      {children({ id, 'aria-invalid': invalid, 'aria-describedby': invalid ? errorId : undefined })}
      {invalid && <FieldError id={errorId} errors={meta.errors} />}
    </Field>
  )
}

export function CfstSettings({ form }: { form: CfstSettingsForm }) {
  const mode = useStore(form.store, (state) => state.values.mode)
  const download = useStore(form.store, (state) => state.values.download)

  function numberField(name: NumberSetting, label: string, placeholder?: string) {
    const disabled = !isCfstSettingEnabled(name, { mode, download })
    return (
      <form.Field name={name}>
        {(field) => (
          <SettingField label={label} flag={name} disabled={disabled} meta={field.state.meta}>
            {({ id, ...aria }) => (
              <InputNumber
                id={id}
                {...numberSettings[name]}
                disabled={disabled}
                value={Number.isNaN(field.state.value) ? null : field.state.value}
                onBlur={field.handleBlur}
                onValueChange={field.handleChange}
                inputProps={{ ...aria, placeholder }}
              />
            )}
          </SettingField>
        )}
      </form.Field>
    )
  }

  function textField(name: 'cfcolo' | 'url', label: string, placeholder: string) {
    const disabled = !isCfstSettingEnabled(name, { mode, download })
    return (
      <form.Field name={name}>
        {(field) => (
          <SettingField label={label} flag={name} disabled={disabled} meta={field.state.meta}>
            {(inputProps) => (
              <Input
                {...inputProps}
                type={name === 'url' ? 'url' : 'text'}
                placeholder={placeholder}
                disabled={disabled}
                value={field.state.value}
                onBlur={field.handleBlur}
                onChange={(event) => field.handleChange(event.currentTarget.value)}
              />
            )}
          </SettingField>
        )}
      </form.Field>
    )
  }

  return (
    <form
      noValidate
      aria-label="CFST 测速参数"
      className="cfst-settings-form"
      onSubmit={(event) => event.preventDefault()}
    >
      {/* Avoid WebKit remeasuring grid fieldsets inside a flex stack on focus. */}
      <FieldGroup className="cfst-settings-groups grid gap-4">
        <div className={settingsCard}>
          <FieldSet className={`${settingsGridTwo} @min-[42rem]:grid-cols-4`}>
            <FieldLegend className="cfst-settings-group-title col-span-full mb-1 text-xs font-semibold">
              通用测速
            </FieldLegend>
            <form.Field name="mode">
              {(field) => (
                <Field className={settingField}>
                  <FieldLabel
                    id="cfst-mode-label"
                    htmlFor="cfst-mode"
                    className="cfst-setting-label min-w-0"
                  >
                    <span className="cfst-setting-label-text">
                      测速模式{' '}
                      <code className="cfst-setting-flag font-mono text-muted-foreground">
                        -httping
                      </code>
                    </span>
                  </FieldLabel>
                  <Select
                    items={probeModes}
                    value={field.state.value}
                    onValueChange={(value) => {
                      if (value) field.handleChange(value)
                    }}
                  >
                    <SelectTrigger
                      id="cfst-mode"
                      aria-labelledby="cfst-mode-label"
                      className="w-full"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent alignItemWithTrigger={false}>
                      <SelectGroup>
                        {probeModes.map(({ label, value }) => (
                          <SelectItem key={value} value={value}>
                            {label}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                </Field>
              )}
            </form.Field>
            {numberField('tp', '默认端口', '节点端口或 443')}
            {numberField('n', '并发线程数')}
            {numberField('t', '每 IP 测试次数')}
            <div className="cfst-url-field col-span-full">
              {textField('url', 'HTTPing / 下载测速 URL', defaultCfstUrl)}
            </div>
          </FieldSet>
        </div>

        <div className={settingsCard}>
          <FieldSet className={settingsGridTwo}>
            <FieldLegend className="cfst-settings-group-title col-span-full mb-1 text-xs font-semibold">
              HTTPing 筛选
            </FieldLegend>
            {numberField('httping-code', 'HTTP 状态码', '例如 200')}
            {textField('cfcolo', '地区过滤', '例如 HKG,NRT')}
          </FieldSet>
        </div>

        <div className={settingsCard}>
          <FieldSet className={settingsGridThree}>
            <FieldLegend className="cfst-settings-group-title col-span-full mb-1 text-xs font-semibold">
              延迟筛选
            </FieldLegend>
            {numberField('tll', '最低延迟（ms）')}
            {numberField('tl', '最高延迟（ms）')}
            {numberField('tlr', '丢包率上限（0–1）')}
          </FieldSet>
        </div>

        <div className={settingsCard}>
          <FieldSet className={settingsGridThree}>
            <div className="cfst-download-header col-span-full flex items-center justify-between gap-3">
              <FieldLegend className="cfst-settings-group-title mb-0 text-xs font-semibold">
                下载测速
              </FieldLegend>
              <form.Field name="download">
                {(field) => (
                  <Switch
                    id="cfst-download"
                    aria-label="启用下载测速"
                    checked={field.state.value}
                    onCheckedChange={field.handleChange}
                  />
                )}
              </form.Field>
            </div>
            {numberField('dn', '候选数量')}
            {numberField('dt', '每 IP 时限（秒）')}
            {numberField('sl', '最低速度（MB/s）')}
          </FieldSet>
        </div>
      </FieldGroup>
    </form>
  )
}
