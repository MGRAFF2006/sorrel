import { defineConfig } from 'eslint/config';
import expoConfig from 'eslint-config-expo/flat.js';

export default defineConfig([
  expoConfig,
  {
    ignores: ['dist/**', '.expo/**'],
    rules: {
      'import/order': ['warn', { alphabetize: { order: 'asc' } }],
    },
  },
]);
