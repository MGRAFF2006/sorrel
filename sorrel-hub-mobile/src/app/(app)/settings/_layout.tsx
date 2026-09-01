import { Stack } from 'expo-router';

import { useSorrelTheme } from '@/lib/theme';

export default function SettingsLayout() {
  const theme = useSorrelTheme();
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: theme.colors.surface },
        headerTintColor: theme.colors.text,
        headerShadowVisible: false,
        contentStyle: { backgroundColor: theme.colors.background },
      }}
    />
  );
}
