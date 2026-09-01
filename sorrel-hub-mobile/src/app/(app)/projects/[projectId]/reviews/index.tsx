import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import { FlatList, Pressable, ScrollView, Text, View } from 'react-native';

import { EmptyState, ErrorState, ListRow, LoadingState, StatusPill } from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { unwrapList } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';
import type { Proposal } from '@/lib/types';

const FILTERS = ['all', 'open', 'draft', 'approved', 'merged', 'closed'] as const;

export default function ReviewsScreen() {
  const router = useRouter();
  const theme = useSorrelTheme();
  const { projectId: rawProjectId } = useLocalSearchParams<{ projectId: string }>();
  const projectId = String(rawProjectId);
  const { client } = useHub();
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState<(typeof FILTERS)[number]>('all');
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    return unwrapList<Proposal>(await client.listProposals({ projectId }));
  }, [client, projectId]);
  const query = useHubQuery(load);
  const proposals = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (query.data ?? []).filter((proposal) => {
      if (status !== 'all' && proposal.status !== status) return false;
      if (!needle) return true;
      return [proposal.title, proposal.id, proposal.sourceLane, proposal.syncRepoId]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(needle);
    });
  }, [query.data, search, status]);

  if (query.loading) return <LoadingState label="Loading reviews…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;

  return (
    <>
      <Stack.Screen
        options={{
          title: 'Reviews',
          headerLargeTitle: true,
          headerSearchBarOptions: {
            placeholder: 'Find reviews',
            onChangeText: (event) => setSearch(event.nativeEvent.text),
            onCancelButtonPress: () => setSearch(''),
          },
          headerRight: () => (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Open review"
              hitSlop={10}
              onPress={() =>
                router.push({ pathname: '/projects/[projectId]/reviews/new', params: { projectId } })
              }
            >
              <Text style={{ color: theme.colors.accent, fontSize: 28 }}>＋</Text>
            </Pressable>
          ),
        }}
      />
      <FlatList
        data={proposals}
        keyExtractor={(proposal) => proposal.id}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{
          width: '100%',
          maxWidth: 920,
          alignSelf: 'center',
          paddingHorizontal: 14,
          paddingBottom: 48,
          gap: 9,
          flexGrow: 1,
        }}
        refreshing={query.refreshing}
        onRefresh={() => void query.reload()}
        ListHeaderComponent={
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            contentContainerStyle={{ gap: 8, paddingVertical: 12 }}
          >
            {FILTERS.map((value) => {
              const selected = status === value;
              return (
                <Pressable
                  key={value}
                  accessibilityRole="radio"
                  accessibilityState={{ selected }}
                  onPress={() => setStatus(value)}
                  style={({ pressed }) => ({
                    borderRadius: 999,
                    paddingHorizontal: 14,
                    paddingVertical: 8,
                    backgroundColor: selected ? theme.colors.accentSoft : theme.colors.surface,
                    borderWidth: 1,
                    borderColor: selected ? theme.colors.accent : theme.colors.border,
                    opacity: pressed ? 0.7 : 1,
                  })}
                >
                  <Text style={{ color: theme.colors.text, fontWeight: '700', textTransform: 'capitalize' }}>
                    {value}
                  </Text>
                </Pressable>
              );
            })}
          </ScrollView>
        }
        ListEmptyComponent={
          <EmptyState
            title={(query.data ?? []).length === 0 ? 'No reviews yet' : 'No matching reviews'}
            body={(query.data ?? []).length === 0 ? 'Submit a lane from the CLI or open a review here.' : 'Try another search or lifecycle filter.'}
          />
        }
        renderItem={({ item }) => (
          <View style={{ marginBottom: 1 }}>
            <ListRow
              title={item.title ?? item.id}
              subtitle={[item.sourceLane ? `lane · ${item.sourceLane}` : undefined, item.syncRepoId]
                .filter(Boolean)
                .join(' · ')}
              detail={<StatusPill value={item.status} />}
              onPress={() =>
                router.push({
                  pathname: '/projects/[projectId]/reviews/[proposalId]',
                  params: { projectId, proposalId: item.id },
                })
              }
            />
          </View>
        )}
      />
    </>
  );
}
