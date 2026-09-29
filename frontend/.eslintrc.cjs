/* eslint-env node */
require('@rushstack/eslint-patch/modern-module-resolution')

module.exports = {
  root: true,
  extends: [
    'plugin:vue/vue3-essential',
    'eslint:recommended',
    '@vue/eslint-config-typescript',
    '@vue/eslint-config-prettier/skip-formatting'
  ],
  parserOptions: {
    ecmaVersion: 'latest'
  },
  rules: {
    'no-empty': ['error', { allowEmptyCatch: true }],
    'vue/no-use-v-if-with-v-for': 'off',
    'vue/valid-template-root': 'off',
    'vue/no-unused-components': 'off'
  },
  overrides: [
    {
      files: ['tests/**/*.{js,mjs,ts}', '*.config.{js,cjs,mjs,ts}'],
      env: {
        node: true
      }
    }
  ]
}
