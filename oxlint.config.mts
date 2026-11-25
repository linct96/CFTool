import { tanstackConfig } from '@tanstack/oxlint-config'
import { defineConfig } from 'oxlint'

export default defineConfig({
  ...tanstackConfig,
  plugins: [...(tanstackConfig.plugins ?? []), 'react', 'jsx-a11y'],
  categories: { correctness: 'error' },
  ignorePatterns: [
    ...tanstackConfig.ignorePatterns,
    'dist-ssr/**',
    'src-tauri/target/**',
    'src-tauri/gen/**',
    '.*/**',
  ],
  rules: {
    ...tanstackConfig.rules,
    'typescript/array-type': ['error', { default: 'array', readonly: 'array' }],
    'typescript/no-unnecessary-condition': 'error',
    'typescript/consistent-type-imports': ['error', { prefer: 'type-imports' }],
    'typescript/no-unnecessary-type-assertion': 'error',
    'typescript/unbound-method': 'error',
    'unicorn/no-useless-spread': 'error',
    'react/rules-of-hooks': 'error',
    'react/exhaustive-deps': 'error',
  },
})
