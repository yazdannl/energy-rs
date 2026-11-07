// ESLint 9 flat config: recommended JS + TypeScript + Svelte rules, with `any` as an error.
//
// The UI is small but it ships inside the add-on: an implicit `any` there is a contract with the
// REST API that nobody checked, so it fails the build instead of warning.
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
  {
    ignores: ['dist/', 'node_modules/'],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: {
      globals: { ...globals.browser },
    },
  },
  {
    // TypeScript inside `<script lang="ts">` goes through the TS parser, not the JS one.
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      parserOptions: {
        parser: ts.parser,
      },
    },
  },
  {
    // Build tooling runs in Node, not in the browser.
    files: ['*.config.js', '*.config.ts', 'vite.config.ts', 'svelte.config.js'],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
    },
  },
);
