import { Stack, ThemeProvider } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import * as SystemUI from 'expo-system-ui';
import { useEffect } from 'react';

import { HubProvider } from '@/context/hub-context';
import { navigationTheme, useSorrelTheme } from '@/lib/theme';

export default function RootLayout() {
  const theme = useSorrelTheme();

  useEffect(() => {
    void SystemUI.setBackgroundColorAsync(theme.colors.background);
  }, [theme.colors.background]);

  return (
    <HubProvider>
      <ThemeProvider value={navigationTheme(theme)}>
        <StatusBar style={theme.dark ? 'light' : 'dark'} />
        <Stack
          screenOptions={{
            headerStyle: { backgroundColor: theme.colors.surface },
            headerTintColor: theme.colors.text,
            headerShadowVisible: false,
            contentStyle: { backgroundColor: theme.colors.background },
          }}
        >
          <Stack.Screen name="index" options={{ headerShown: false }} />
          <Stack.Screen name="(app)" options={{ headerShown: false }} />
          <Stack.Screen
            name="connect"
            options={{
              title: 'Connect to Hub',
            }}
          />
        </Stack>
      </ThemeProvider>
    </HubProvider>
  );
}
