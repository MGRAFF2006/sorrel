import { NativeTabs } from 'expo-router/unstable-native-tabs';

import { useSorrelTheme } from '@/lib/theme';

export default function AppTabs() {
  const theme = useSorrelTheme();
  return (
    <NativeTabs
      sidebarAdaptable
      backBehavior="history"
      backgroundColor={theme.colors.surface}
      iconColor={{ default: theme.colors.muted, selected: theme.colors.accent }}
      tintColor={theme.colors.accent}
      indicatorColor={theme.colors.accentSoft}
      labelVisibilityMode="labeled"
      tabBarRespectsIMEInsets
      minimizeBehavior="onScrollDown"
    >
      <NativeTabs.Trigger name="projects">
        <NativeTabs.Trigger.Icon
          sf={{ default: 'square.stack.3d.up', selected: 'square.stack.3d.up.fill' }}
          md={{ default: 'workspaces', selected: 'workspaces' }}
        />
        <NativeTabs.Trigger.Label>Projects</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Icon
          sf={{ default: 'gearshape', selected: 'gearshape.fill' }}
          md={{ default: 'settings', selected: 'settings' }}
        />
        <NativeTabs.Trigger.Label>Settings</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
