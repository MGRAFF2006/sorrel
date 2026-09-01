import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback } from 'react';
import { Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';

import {
  Body,
  Card,
  Content,
  EmptyState,
  ErrorState,
  Eyebrow,
  LoadingState,
  Mono,
  Page,
  StatusPill,
  Title,
} from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { tabletColumns } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';
import type { DataResponse, Project, ProposalSummary } from '@/lib/types';

export default function ProjectOverviewScreen() {
  const router = useRouter();
  const theme = useSorrelTheme();
  const { width } = useWindowDimensions();
  const columns = tabletColumns(width);
  const { projectId: rawProjectId } = useLocalSearchParams<{ projectId: string }>();
  const projectId = String(rawProjectId);
  const { client } = useHub();
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    const [project, summary] = await Promise.all([
      client.getProject<DataResponse<Project>>(projectId),
      client.proposalSummary<DataResponse<ProposalSummary>>({ projectId }),
    ]);
    return { project: project.data, summary: summary.data };
  }, [client, projectId]);
  const query = useHubQuery(load);

  if (query.loading) return <LoadingState label="Loading project…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;
  if (!query.data) return <EmptyState title="Project unavailable" body="No project data was returned." />;

  const { project, summary } = query.data;
  const openReviews = (summary.byStatus?.open ?? 0) + (summary.byStatus?.draft ?? 0);
  const actions = [
    {
      eyebrow: 'Review',
      title: `${openReviews} open review${openReviews === 1 ? '' : 's'}`,
      body: 'Inspect proposals, discuss changes, and update their lifecycle.',
      onPress: () =>
        router.push({ pathname: '/projects/[projectId]/reviews', params: { projectId } }),
    },
    {
      eyebrow: 'Sync',
      title: 'Repositories & refs',
      body: 'See project-connected sync repositories and their current snapshot tips.',
      onPress: () =>
        router.push({ pathname: '/projects/[projectId]/repositories', params: { projectId } }),
    },
  ];

  return (
    <Page>
      <Stack.Screen options={{ title: project.name ?? 'Project' }} />
      <Content>
        <View style={{ gap: 9 }}>
          <Eyebrow>{project.organizationId ?? 'Project'}</Eyebrow>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
            <View style={{ flex: 1 }}>
              <Title>{project.name ?? project.id}</Title>
            </View>
            <StatusPill value={project.status} />
          </View>
          <Body muted>{project.description ?? 'Review changes and keep local workspaces moving together.'}</Body>
        </View>

        <View style={[styles.grid, columns === 1 && styles.gridSingle]}>
          {actions.map((action) => (
            <Pressable
              key={action.title}
              accessibilityRole="button"
              onPress={action.onPress}
              style={({ pressed }) => [styles.actionWrap, pressed && { opacity: 0.72 }]}
            >
              <Card style={{ flex: 1 }}>
                <Eyebrow>{action.eyebrow}</Eyebrow>
                <Text style={{ color: theme.colors.text, fontSize: 22, fontWeight: '800' }}>
                  {action.title}
                </Text>
                <Body muted>{action.body}</Body>
                <Text style={{ color: theme.colors.accent, fontSize: 22 }}>→</Text>
              </Card>
            </Pressable>
          ))}
        </View>

        <Card>
          <Body muted>Project id</Body>
          <Mono>{project.id}</Mono>
          <Body muted>
            Policy and grant references remain owned by Sorrel Core. This app displays Hub state but does not decide access.
          </Body>
        </Card>
      </Content>
    </Page>
  );
}

const styles = StyleSheet.create({
  grid: { flexDirection: 'row', gap: 12 },
  gridSingle: { flexDirection: 'column' },
  actionWrap: { flex: 1 },
});
