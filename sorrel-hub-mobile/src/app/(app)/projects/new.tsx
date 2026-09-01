import * as Haptics from 'expo-haptics';
import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { KeyboardAvoidingView, Platform } from 'react-native';

import { Body, Button, Content, Field, Notice, Page } from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { formatError } from '@/lib/domain';
import type { DataResponse, Project } from '@/lib/types';

export default function NewProjectScreen() {
  const router = useRouter();
  const { client } = useHub();
  const [organizationId, setOrganizationId] = useState('');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');

  async function createProject() {
    if (!client || !organizationId.trim() || !name.trim()) return;
    setError('');
    setSubmitting(true);
    try {
      const result = await client.createProject<DataResponse<Project>>({
        organizationId: organizationId.trim(),
        name: name.trim(),
        description: description.trim() || undefined,
      });
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
      router.dismissTo({ pathname: '/projects/[projectId]', params: { projectId: result.data.id } });
    } catch (caught) {
      setError(formatError(caught));
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Stack.Screen options={{ title: 'New project' }} />
      <Page>
        <Content>
          <Body muted>A shared home for reviews, repositories, and local workspaces.</Body>
          <Field
            label="Organization"
            value={organizationId}
            onChangeText={setOrganizationId}
            autoCapitalize="none"
            autoCorrect={false}
            placeholder="your-team"
          />
          <Field
            label="Project name"
            value={name}
            onChangeText={setName}
            autoCorrect={false}
            placeholder="Acme platform"
          />
          <Field
            label="Description (optional)"
            value={description}
            onChangeText={setDescription}
            multiline
            placeholder="What is this project for?"
          />
          {error ? <Notice tone="danger">{error}</Notice> : null}
          <Button
            label={submitting ? 'Creating…' : 'Create project'}
            disabled={submitting || !organizationId.trim() || !name.trim()}
            onPress={createProject}
          />
        </Content>
      </Page>
    </KeyboardAvoidingView>
  );
}
