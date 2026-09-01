import { useColorScheme } from 'react-native';

export type SorrelTheme = {
  dark: boolean;
  colors: {
    background: string;
    surface: string;
    surfaceRaised: string;
    border: string;
    text: string;
    muted: string;
    accent: string;
    accentSoft: string;
    success: string;
    warning: string;
    danger: string;
    input: string;
  };
};

const dark: SorrelTheme = {
  dark: true,
  colors: {
    background: '#242933',
    surface: '#2E3440',
    surfaceRaised: '#3B4252',
    border: '#4C566A',
    text: '#ECEFF4',
    muted: '#B6BFCE',
    accent: '#88C0D0',
    accentSoft: '#344754',
    success: '#A3BE8C',
    warning: '#EBCB8B',
    danger: '#BF616A',
    input: '#252B35',
  },
};

const light: SorrelTheme = {
  dark: false,
  colors: {
    background: '#F4F6F8',
    surface: '#FFFFFF',
    surfaceRaised: '#ECEFF4',
    border: '#D8DEE9',
    text: '#2E3440',
    muted: '#667085',
    accent: '#4C729F',
    accentSoft: '#DCEAF1',
    success: '#5D7B48',
    warning: '#9A6A1F',
    danger: '#A94F59',
    input: '#FFFFFF',
  },
};

export function useSorrelTheme(): SorrelTheme {
  return useColorScheme() === 'dark' ? dark : light;
}

export function navigationTheme(theme: SorrelTheme) {
  return {
    dark: theme.dark,
    colors: {
      primary: theme.colors.accent,
      background: theme.colors.background,
      card: theme.colors.surface,
      text: theme.colors.text,
      border: theme.colors.border,
      notification: theme.colors.danger,
    },
    fonts: {
      regular: { fontFamily: 'System', fontWeight: '400' as const },
      medium: { fontFamily: 'System', fontWeight: '500' as const },
      bold: { fontFamily: 'System', fontWeight: '700' as const },
      heavy: { fontFamily: 'System', fontWeight: '800' as const },
    },
  };
}
