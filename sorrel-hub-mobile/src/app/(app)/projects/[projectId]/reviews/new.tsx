import * as Haptics from 'expo-haptics';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { KeyboardAvoidingView, Platform } from 'react-native';

import { Body, Button, Content, Field, Notice, Page } from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { formatError } from '@/lib/domain';
import type { DataResponse, Proposal } from '@/lib/types';

export default function NewReviewScreen() {
  const router = useRouter();
  const { projectId: rawProjectId } = useLocalSearchParams<{ projectId: string }>();
  const projectId = String(rawProjectId);
  const { client, connection } = useHub();
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [syncRepoId, setSyncRepoId] = useState('');
  const [sourceLane, setSourceLane] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');

  async function createReview() {
    if (!client || !connection || !title.trim()) return;
    setSubmitting(true);
    setError('');
    try {
      const result = await client.createProposal<DataResponse<Proposal>>({
        projectId,
        title: title.trim(),
        description: description.trim() || undefined,
        syncRepoId: syncRepoId.trim() || undefined,
        sourceLane: sourceLane.trim() || undefined,
        authorPrincipal: connection.principal,
        status: 'open',
      });
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
      router.dismissTo({
        pathname: '/projects/[projectId]/reviews/[proposalId]',
        params: { projectId, proposalId: result.data.id },
      });
    } catch (caught) {
      setError(formatError(caught));
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Stack.Screen options={{ title: 'Open review' }} />
      <Page>
        <Content>
          <Body muted>Bring a lane into the project for discussion and approval.</Body>
          <Field label="Title" value={title} onChangeText={setTitle} placeholder="Land feature lane" />
          <Field
            label="Description (optional)"
            value={description}
            onChangeText={setDescription}
            multiline
            placeholder="What changed, and where should reviewers focus?"
          />
          <Field
            label="Sync repository (optional)"
            value={syncRepoId}
            onChangeText={setSyncRepoId}
            autoCapitalize="none"
            autoCorrect={false}
            placeholder="repo_…"
          />
          <Field
            label="Source lane (optional)"
            value={sourceLane}
            onChangeText={setSourceLane}
            autoCapitalize="none"
            autoCorrect={false}
            placeholder="lane_feature"
          />
          {error ? <Notice tone="danger">{error}</Notice> : null}
          <Button label={submitting ? 'Opening…' : 'Open review'} disabled={submitting || !title.trim()} onPress={createReview} />
        </Content>
      </Page>
    </KeyboardAvoidingView>
  );
}
