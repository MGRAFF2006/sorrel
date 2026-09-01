import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback } from 'react';
import { Pressable, Text, View } from 'react-native';

import {
  Body,
  Card,
  Content,
  EmptyState,
  ErrorState,
  ListRow,
  LoadingState,
  Mono,
  Page,
  Section,
} from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { unwrapList } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';
import type { Proposal, Repository, SyncRepo } from '@/lib/types';

export default function RepositoriesScreen() {
  const router = useRouter();
  const theme = useSorrelTheme();
  const { projectId: rawProjectId } = useLocalSearchParams<{ projectId: string }>();
  const projectId = String(rawProjectId);
  const { client } = useHub();
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    const [syncPayload, proposalPayload, repositoryPayload] = await Promise.all([
      client.listSyncRepos(),
      client.listProposals({ projectId }),
      client.listRepositories({ projectId }),
    ]);
    const allSyncRepos = unwrapList<SyncRepo>(syncPayload);
    const proposals = unwrapList<Proposal>(proposalPayload);
    const repositories = unwrapList<Repository>(repositoryPayload);
    const linked = new Set(
      proposals
        .map((proposal) => proposal.syncRepoId)
        .filter((id): id is string => typeof id === 'string' && id.length > 0),
    );
    return {
      repositories,
      syncRepos: allSyncRepos.filter((repo) => linked.has(repo.id)),
    };
  }, [client, projectId]);
  const query = useHubQuery(load);

  if (query.loading) return <LoadingState label="Loading repositories…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;

  const repositories = query.data?.repositories ?? [];
  const syncRepos = query.data?.syncRepos ?? [];
  return (
    <Page>
      <Stack.Screen
        options={{
          title: 'Repositories',
          headerLargeTitle: true,
          headerRight: () => (
            <Pressable accessibilityRole="button" onPress={() => void query.reload()} hitSlop={10}>
              <Text style={{ color: theme.colors.accent, fontWeight: '700' }}>Refresh</Text>
            </Pressable>
          ),
        }}
      />
      <Content>
        <Section title="Project sources">
          {repositories.length ? (
            repositories.map((repository) => (
              <Card key={repository.id}>
                <Body>{repository.name ?? repository.id}</Body>
                <Mono muted>{repository.id}</Mono>
                <Body muted>
                  {[repository.provider, repository.owner].filter(Boolean).join(' · ') || 'Sorrel repository'}
                </Body>
              </Card>
            ))
          ) : (
            <Card><Body muted>No administrative repository records are attached to this project.</Body></Card>
          )}
        </Section>

        <Section title="Content-addressed sync">
          {syncRepos.length ? (
            <View style={{ gap: 9 }}>
              {syncRepos.map((repo) => (
                <ListRow
                  key={repo.id}
                  title={repo.id}
                  subtitle={`${repo.refCount ?? 0} named ref${repo.refCount === 1 ? '' : 's'}`}
                  onPress={() =>
                    router.push({
                      pathname: '/projects/[projectId]/repositories/[repoId]',
                      params: { projectId, repoId: repo.id },
                    })
                  }
                />
              ))}
            </View>
          ) : (
            <EmptyState
              title="No sync repository connected"
              body="Push a local workspace or submit a lane to connect its repository to this project."
            />
          )}
        </Section>

        <Card>
          <Body muted>
            VCS objects remain in Sorrel’s sync object store. This app reads refs and collaboration metadata; it does not move objects into product metadata.
          </Body>
        </Card>
      </Content>
    </Page>
  );
}
