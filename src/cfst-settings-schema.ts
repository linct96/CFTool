import { z } from 'zod'

export const defaultCfstUrl = 'https://cf.xiu2.xyz/url'

const numericDraft = z.union([z.number(), z.null(), z.nan()])
const draftSchema = z.object({
  mode: z.enum(['tcp', 'http']),
  download: z.boolean(),
  tp: numericDraft,
  n: numericDraft,
  t: numericDraft,
  'httping-code': numericDraft,
  cfcolo: z.string(),
  url: z.string(),
  tll: numericDraft,
  tl: numericDraft,
  tlr: numericDraft,
  dn: numericDraft,
  dt: numericDraft,
  sl: numericDraft,
})

export type CfstSettingsDraft = z.infer<typeof draftSchema>

export const defaultCfstSettings = {
  mode: 'tcp',
  download: false,
  tp: null,
  n: 200,
  t: 4,
  'httping-code': null,
  cfcolo: '',
  url: defaultCfstUrl,
  tll: 0,
  tl: 9999,
  tlr: 0,
  dn: 10,
  dt: 10,
  sl: 0,
} satisfies CfstSettingsDraft

export const numberSettings = {
  tp: { min: 1, max: 65535, step: 1 },
  n: { min: 1, max: 1000, step: 1 },
  t: { min: 1, max: 100, step: 1 },
  'httping-code': { min: 100, max: 599, step: 1 },
  tll: { min: 0, max: 300000, step: 1 },
  tl: { min: 0, max: 300000, step: 1 },
  tlr: { min: 0, max: 1, step: 'any' },
  dn: { min: 1, max: 1000, step: 1 },
  dt: { min: 1, max: 600, step: 1 },
  sl: { min: 0, max: 100000, step: 'any' },
} as const

export type NumberSetting = keyof typeof numberSettings
type OptionalSetting = NumberSetting | 'url' | 'cfcolo'

export function isCfstSettingEnabled(
  key: OptionalSetting,
  { mode, download }: Pick<CfstSettingsDraft, 'mode' | 'download'>,
) {
  if (key === 'url') return mode === 'http' || download
  if (key === 'httping-code' || key === 'cfcolo') return mode === 'http'
  if (key === 'dn' || key === 'dt' || key === 'sl') return download
  return true
}

function isCfstUrl(value: string) {
  if (!/^https?:\/\/[^/\\?#]/i.test(value) || /[\s\\]/.test(value)) return false
  try {
    const url = new URL(value)
    return Boolean(url.hostname) && !url.username && !url.password
  } catch {
    return false
  }
}

const numericKeys = Object.keys(numberSettings) as NumberSetting[]

export const cfstSettingsSchema = draftSchema.superRefine((values, context) => {
  function issue(key: OptionalSetting, message: string) {
    context.addIssue({ code: 'custom', path: [key], message })
  }

  for (const key of numericKeys) {
    const value = values[key]
    if (value === null || !isCfstSettingEnabled(key, values)) continue
    const { min, max, step } = numberSettings[key]
    if (Number.isNaN(value)) issue(key, '请输入有效数字。')
    else if (value < min || value > max || (step !== 'any' && !Number.isInteger(value)))
      issue(key, `请输入 ${min}–${max} 之间的${step === 'any' ? '数字' : '整数'}。`)
  }

  if ((values.tll ?? defaultCfstSettings.tll) > (values.tl ?? defaultCfstSettings.tl))
    issue('tl', '最高延迟不能低于最低延迟。')

  const colo = values.cfcolo.trim()
  if (
    isCfstSettingEnabled('cfcolo', values) &&
    colo &&
    !/^[A-Za-z]{2,3}(?:,[A-Za-z]{2,3})*$/.test(colo)
  )
    issue('cfcolo', '请输入逗号分隔的地区代码，例如 HKG,NRT。')

  const url = values.url.trim()
  if (isCfstSettingEnabled('url', values) && url && !isCfstUrl(url))
    issue('url', '请输入有效的 HTTP 或 HTTPS 地址，不能包含账号密码或空白字符。')
})

export const storedCfstSettingsSchema = draftSchema
  .partial()
  .transform((values) => ({ ...defaultCfstSettings, ...values }))
  .pipe(cfstSettingsSchema)

export function toSavedCfstSettings(values: CfstSettingsDraft) {
  if (!cfstSettingsSchema.safeParse(values).success) return null

  // Keep valid inactive settings, but never persist invalid drafts from disabled fields.
  const allFields = cfstSettingsSchema.safeParse({ ...values, mode: 'http', download: true })
  if (allFields.success) return JSON.stringify(values)
  const invalidFields = new Set(allFields.error.issues.map(({ path }) => path[0]))
  return JSON.stringify(values, (key, value: unknown) =>
    invalidFields.has(key) ? undefined : value,
  )
}

export function toProbeSettings(values: CfstSettingsDraft) {
  const settings: Record<string, string> = {
    mode: values.mode,
    download: values.download ? 'yes' : 'no',
  }
  for (const key of [...numericKeys, 'cfcolo', 'url'] as const) {
    if (!isCfstSettingEnabled(key, values)) continue
    const value = values[key]
    if (value !== null && String(value).trim() !== '') settings[key] = String(value).trim()
  }
  return settings
}
