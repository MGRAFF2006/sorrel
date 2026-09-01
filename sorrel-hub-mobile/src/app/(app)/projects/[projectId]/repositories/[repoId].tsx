import { Stack, useLocalSearchParams } from 'expo-router';
import { useCallback } from 'react';
import { View } from 'react-native';

import {
  Body,
  Card,
  Content,
  EmptyState,
  ErrorState,
  LoadingState,
  Mono,
  Page,
  Section,
} from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { shortId, unwrapList } from '@/lib/domain';
import type { SyncRef } from '@/lib/types';

export default function RepositoryRefsScreen() {
  const { repoId: rawRepoId } = useLocalSearchParams<{ repoId: string }>();
  const repoId = String(rawRepoId);
  const { client } = useHub();
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    return unwrapList<SyncRef>(await client.listRefs(repoId));
  }, [client, repoId]);
  const query = useHubQuery(load);

  if (query.loading) return <LoadingState label="Loading refs…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;

  const refs = query.data ?? [];
  return (
    <Page>
      <Stack.Screen options={{ title: 'Repository refs' }} />
      <Content>
        <Card>
          <Body muted>Repository</Body>
          <Mono>{repoId}</Mono>
        </Card>
        <Section title={`${refs.length} ref${refs.length === 1 ? '' : 's'}`}>
          {refs.length ? (
            <View style={{ gap: 9 }}>
              {refs.map((ref, index) => (
                <Card key={`${ref.name ?? 'ref'}-${index}`}>
                  <Body>{ref.name ?? 'Unnamed ref'}</Body>
                  <Mono muted>{shortId(ref.snapshot, 18)}</Mono>
                  {ref.snapshot ? <Mono>{ref.snapshot}</Mono> : null}
                </Card>
              ))}
            </View>
          ) : (
            <EmptyState title="No refs" body="This repository has no named refs yet." />
          )}
        </Section>
      </Content>
    </Page>
  );
}
