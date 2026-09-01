import * as Haptics from 'expo-haptics';
import { Stack, useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { Alert, Pressable, Text, View } from 'react-native';

import {
  Body,
  Button,
  Card,
  Content,
  ErrorState,
  ListRow,
  LoadingState,
  Mono,
  Notice,
  Page,
  Section,
  StatusPill,
} from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { useHubQuery } from '@/hooks/use-hub-query';
import { useSorrelTheme } from '@/lib/theme';
import type { DataResponse, HubCapabilities, HubSession, Principal } from '@/lib/types';

const DEV_PRINCIPALS: Principal[] = [
  { type: 'user', id: 'local' },
  { type: 'user', id: 'reviewer' },
  { type: 'user', id: 'maintainer' },
  { type: 'agent', id: 'ci' },
];

export default function SettingsScreen() {
  const router = useRouter();
  const theme = useSorrelTheme();
  const { client, connection, hasAccessToken, save, disconnect } = useHub();
  const [changingIdentity, setChangingIdentity] = useState(false);
  const load = useCallback(async () => {
    if (!client) throw new Error('No Hub connection.');
    const [capabilities, session] = await Promise.all([
      client.capabilities<DataResponse<HubCapabilities>>(),
      client.session<DataResponse<HubSession>>(),
    ]);
    return { capabilities: capabilities.data, session: session.data };
  }, [client]);
  const query = useHubQuery(load);

  async function choosePrincipal(principal: Principal) {
    if (!connection) return;
    await save({
      baseUrl: connection.baseUrl,
      principal,
      preserveAccessToken: true,
    });
    setChangingIdentity(false);
    await Haptics.selectionAsync();
    await query.reload();
  }

  function confirmDisconnect() {
    Alert.alert('Disconnect from Hub?', 'The saved endpoint and bearer credential will be removed from this device.', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Disconnect',
        style: 'destructive',
        onPress: () => {
          void disconnect().then(() => router.replace('/connect'));
        },
      },
    ]);
  }

  if (query.loading) return <LoadingState label="Reading Hub settings…" />;
  if (query.error) return <ErrorState error={query.error} retry={() => void query.reload()} />;
  const capabilities = query.data?.capabilities;
  const session = query.data?.session;
  const effectivePrincipal = session?.session?.principal ?? connection?.principal;

  return (
    <Page>
      <Stack.Screen options={{ title: 'Settings', headerLargeTitle: true }} />
      <Content>
        <Section title="Connection">
          <Card>
            <View style={{ gap: 5 }}>
              <Body>Hub endpoint</Body>
              <Mono>{connection?.baseUrl ?? 'Not connected'}</Mono>
            </View>
            <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
              <Body muted>Deployment</Body>
              <StatusPill value={capabilities?.deploy} />
            </View>
            <Body muted>
              {hasAccessToken ? 'Bearer credential saved securely' : 'No bearer credential saved'}
            </Body>
          </Card>
          <ListRow title="Edit Hub connection" subtitle="Endpoint, bearer token, and development identity" onPress={() => router.push('/connect')} />
        </Section>

        <Section title="Identity">
          <Card>
            <Body muted>Effective principal</Body>
            <Mono>{effectivePrincipal ? `${effectivePrincipal.type}:${effectivePrincipal.id}` : 'anonymous'}</Mono>
            <Body muted>Authentication: {capabilities?.auth.mode ?? 'unknown'}</Body>
          </Card>
          {capabilities?.auth.mode === 'dev' ? (
            <>
              <Notice tone="warning">
                Development identities are request hints for local testing, not signed user identity.
              </Notice>
              <Button
                label={changingIdentity ? 'Hide identities' : 'Change development identity'}
                kind="secondary"
                onPress={() => setChangingIdentity((value) => !value)}
              />
              {changingIdentity ? (
                <View style={{ gap: 8 }}>
                  {DEV_PRINCIPALS.map((principal) => {
                    const label = `${principal.type}:${principal.id}`;
                    const selected = label === `${connection?.principal.type}:${connection?.principal.id}`;
                    return (
                      <Pressable
                        key={label}
                        accessibilityRole="radio"
                        accessibilityState={{ selected }}
                        onPress={() => void choosePrincipal(principal)}
                        style={({ pressed }) => ({
                          minHeight: 48,
                          borderRadius: 13,
                          paddingHorizontal: 14,
                          justifyContent: 'center',
                          backgroundColor: selected ? theme.colors.accentSoft : theme.colors.surface,
                          opacity: pressed ? 0.7 : 1,
                        })}
                      >
                        <Text style={{ color: theme.colors.text, fontWeight: '700' }}>{label}</Text>
                      </Pressable>
                    );
                  })}
                </View>
              ) : null}
            </>
          ) : null}
        </Section>

        <Section title="Hub modules">
          <Card>
            <Body>Object storage: {capabilities?.modules.objectStorage ?? 'unknown'}</Body>
            <Body muted>
              Optional modules are shown only when the Hub advertises them. This client never invents permissions or capabilities.
            </Body>
          </Card>
        </Section>

        <Button label="Disconnect" kind="danger" onPress={confirmDisconnect} />
      </Content>
    </Page>
  );
}
