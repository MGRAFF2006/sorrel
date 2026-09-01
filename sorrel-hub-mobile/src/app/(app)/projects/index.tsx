import { Stack, useRouter } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { FlatList, Pressable, Text, useWindowDimensions, View } from 'react-native';

import { EmptyState, ErrorState, ListRow, LoadingState, StatusPill } from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { tabletColumns, unwrapList } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';
import type { Project } from '@/lib/types';

export default function ProjectsScreen() {
  const router = useRouter();
  const theme = useSorrelTheme();
  const { width } = useWindowDimensions();
  const columns = tabletColumns(width);
  const { client } = useHub();
  const [search, setSearch] = useState('');
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    return unwrapList<Project>(await client.listProjects());
  }, [client]);
  const query = useHubQuery(load);
  const projects = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return query.data ?? [];
    return (query.data ?? []).filter((project) =>
      [project.name, project.id, project.organizationId, project.description]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(needle),
    );
  }, [query.data, search]);

  if (query.loading) return <LoadingState label="Loading projects…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;

  return (
    <>
      <Stack.Screen
        options={{
          title: 'Projects',
          headerLargeTitle: true,
          headerSearchBarOptions: {
            placeholder: 'Find projects',
            onChangeText: (event) => setSearch(event.nativeEvent.text),
            onCancelButtonPress: () => setSearch(''),
          },
          headerRight: () => (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Create project"
              hitSlop={10}
              onPress={() => router.push('/projects/new')}
            >
              <Text style={{ color: theme.colors.accent, fontSize: 28, fontWeight: '400' }}>＋</Text>
            </Pressable>
          ),
        }}
      />
      <FlatList
        key={columns}
        data={projects}
        numColumns={columns}
        keyExtractor={(project) => project.id}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{
          width: '100%',
          maxWidth: 1120,
          alignSelf: 'center',
          paddingHorizontal: 12,
          paddingTop: 12,
          paddingBottom: 48,
          gap: 10,
          flexGrow: 1,
        }}
        columnWrapperStyle={columns === 2 ? { gap: 10 } : undefined}
        refreshing={query.refreshing}
        onRefresh={() => void query.reload()}
        ListHeaderComponent={
          <View style={{ paddingHorizontal: 4, paddingBottom: 12, gap: 5 }}>
            <Text style={{ color: theme.colors.text, fontSize: 20, fontWeight: '800' }}>
              Your collaboration spaces
            </Text>
            <Text style={{ color: theme.colors.muted, fontSize: 15, lineHeight: 21 }}>
              Review changes and keep local workspaces in sync.
            </Text>
          </View>
        }
        ListEmptyComponent={
          <EmptyState
            title={search ? 'No matching projects' : 'No projects yet'}
            body={
              search
                ? 'Try a different project, organization, or description.'
                : 'Create a shared home for reviews and repositories.'
            }
          />
        }
        renderItem={({ item }) => (
          <View style={{ flex: 1, marginBottom: 10 }}>
            <ListRow
              title={item.name ?? item.id}
              subtitle={[item.organizationId, item.description].filter(Boolean).join(' · ')}
              detail={<StatusPill value={item.status} />}
              onPress={() =>
                router.push({ pathname: '/projects/[projectId]', params: { projectId: item.id } })
              }
            />
          </View>
        )}
      />
    </>
  );
}
