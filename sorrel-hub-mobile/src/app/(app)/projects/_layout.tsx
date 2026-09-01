import { Stack } from 'expo-router';

import { useSorrelTheme } from '@/lib/theme';

export default function ProjectsLayout() {
  const theme = useSorrelTheme();
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: theme.colors.surface },
        headerTintColor: theme.colors.text,
        headerShadowVisible: false,
        contentStyle: { backgroundColor: theme.colors.background },
        headerBackButtonDisplayMode: 'minimal',
      }}
    >
      <Stack.Screen
        name="new"
        options={{
          title: 'New project',
          presentation: 'formSheet',
          sheetGrabberVisible: true,
          sheetAllowedDetents: [0.72, 1],
        }}
      />
      <Stack.Screen
        name="[projectId]/reviews/new"
        options={{
          title: 'Open review',
          presentation: 'formSheet',
          sheetGrabberVisible: true,
          sheetAllowedDetents: [0.88, 1],
        }}
      />
    </Stack>
  );
}
