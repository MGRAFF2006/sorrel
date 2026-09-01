import * as Haptics from 'expo-haptics';
import { Stack, useLocalSearchParams } from 'expo-router';
import { useCallback, useState } from 'react';
import { Pressable, Text, View } from 'react-native';

import {
  Body,
  Button,
  Card,
  Content,
  EmptyState,
  ErrorState,
  Field,
  LoadingState,
  Mono,
  Notice,
  Page,
  Section,
  StatusPill,
} from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { formatError, PROPOSAL_TRANSITIONS, shortId } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';
import type { DataResponse, Proposal } from '@/lib/types';

export default function ReviewDetailScreen() {
  const theme = useSorrelTheme();
  const { proposalId: rawProposalId } = useLocalSearchParams<{ proposalId: string }>();
  const proposalId = String(rawProposalId);
  const { client, connection } = useHub();
  const [comment, setComment] = useState('');
  const [path, setPath] = useState('');
  const [mutating, setMutating] = useState(false);
  const [mutationError, setMutationError] = useState('');
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    const result = await client.getProposal<DataResponse<Proposal>>(proposalId, {
      includeComments: true,
    });
    return result.data;
  }, [client, proposalId]);
  const query = useHubQuery(load);

  async function updateStatus(status: string) {
    if (!client) return;
    setMutating(true);
    setMutationError('');
    try {
      await client.updateProposal(proposalId, { status });
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
      await query.reload();
    } catch (caught) {
      setMutationError(formatError(caught));
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    } finally {
      setMutating(false);
    }
  }

  async function addComment() {
    if (!client || !connection || !comment.trim()) return;
    setMutating(true);
    setMutationError('');
    try {
      await client.createReviewComment({
        proposalId,
        body: comment.trim(),
        path: path.trim() || undefined,
        authorPrincipal: connection.principal,
      });
      setComment('');
      setPath('');
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
      await query.reload();
    } catch (caught) {
      setMutationError(formatError(caught));
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    } finally {
      setMutating(false);
    }
  }

  async function resolveComment(id: string) {
    if (!client) return;
    setMutating(true);
    setMutationError('');
    try {
      await client.updateReviewComment(id, { state: 'resolved' });
      await Haptics.selectionAsync();
      await query.reload();
    } catch (caught) {
      setMutationError(formatError(caught));
    } finally {
      setMutating(false);
    }
  }

  if (query.loading) return <LoadingState label="Loading review…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;
  if (!query.data) return <EmptyState title="Review unavailable" body="No proposal was returned." />;
  const proposal = query.data;
  const transitions = PROPOSAL_TRANSITIONS[proposal.status ?? ''] ?? [];

  return (
    <Page>
      <Stack.Screen options={{ title: proposal.title ?? 'Review' }} />
      <Content>
        <View style={{ gap: 8 }}>
          <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
            <Text style={{ color: theme.colors.text, fontSize: 24, fontWeight: '800', flex: 1 }}>
              {proposal.title ?? proposal.id}
            </Text>
            <StatusPill value={proposal.status} />
          </View>
          {proposal.description ? <Body>{proposal.description}</Body> : null}
          <Mono muted>{proposal.id}</Mono>
        </View>

        <Card>
          <Body muted>Source lane</Body>
          <Mono>{proposal.sourceLane ?? '—'}</Mono>
          <Body muted>Target lane</Body>
          <Mono>{proposal.targetLane ?? '—'}</Mono>
          <Body muted>Repository</Body>
          <Mono>{proposal.syncRepoId ?? proposal.repositoryId ?? '—'}</Mono>
          {proposal.sourceSnapshot ? (
            <>
              <Body muted>Snapshot</Body>
              <Mono>{shortId(proposal.sourceSnapshot, 18)}</Mono>
            </>
          ) : null}
        </Card>

        {transitions.length ? (
          <Section title="Update status">
            <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 9 }}>
              {transitions.map((status) => (
                <Pressable
                  key={status}
                  accessibilityRole="button"
                  disabled={mutating}
                  onPress={() => void updateStatus(status)}
                  style={({ pressed }) => ({
                    borderRadius: 999,
                    paddingHorizontal: 14,
                    paddingVertical: 9,
                    borderWidth: 1,
                    borderColor: ['rejected', 'closed'].includes(status) ? theme.colors.danger : theme.colors.accent,
                    backgroundColor: theme.colors.surface,
                    opacity: pressed || mutating ? 0.55 : 1,
                  })}
                >
                  <Text style={{ color: theme.colors.text, fontWeight: '700', textTransform: 'capitalize' }}>{status}</Text>
                </Pressable>
              ))}
            </View>
          </Section>
        ) : null}

        <Section title={`Discussion · ${proposal.comments?.length ?? 0}`}>
          {(proposal.comments ?? []).length === 0 ? (
            <Card><Body muted>No review comments yet.</Body></Card>
          ) : (
            (proposal.comments ?? []).map((item) => (
              <Card key={item.id}>
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
                  <Mono muted>{item.authorRef ?? 'reviewer'}</Mono>
                  <View style={{ flex: 1 }} />
                  <StatusPill value={item.state} />
                </View>
                <Body>{item.body ?? ''}</Body>
                {item.path ? <Mono muted>{item.path}{item.line ? `:${item.line}` : ''}</Mono> : null}
                {item.state === 'open' ? (
                  <Button label="Resolve" kind="secondary" disabled={mutating} onPress={() => void resolveComment(item.id)} />
                ) : null}
              </Card>
            ))
          )}
        </Section>

        <Section title="Add comment">
          <Field label="Comment" value={comment} onChangeText={setComment} multiline placeholder="Leave review feedback…" />
          <Field label="File path (optional)" value={path} onChangeText={setPath} autoCapitalize="none" autoCorrect={false} placeholder="src/main.rs" />
          {mutationError ? <Notice tone="danger">{mutationError}</Notice> : null}
          <Button label={mutating ? 'Saving…' : 'Add comment'} disabled={mutating || !comment.trim()} onPress={addComment} />
        </Section>
      </Content>
    </Page>
  );
}
