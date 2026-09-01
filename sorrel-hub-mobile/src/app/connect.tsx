import { HubClient } from '@sorrel/sdk-js';
import * as Haptics from 'expo-haptics';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { Image, KeyboardAvoidingView, Platform, View } from 'react-native';

import { Body, Button, Content, Eyebrow, Field, Notice, Page, Title } from '@/components/ui';
import { useHub } from '@/context/hub-context';
import { formatError, isInsecureConnection, normalizeBaseUrl } from '@/lib/domain';

export default function ConnectScreen() {
  const router = useRouter();
  const { connection, hasAccessToken, save } = useHub();
  const [baseUrl, setBaseUrl] = useState(connection?.baseUrl ?? '');
  const [accessToken, setAccessToken] = useState('');
  const [principalType, setPrincipalType] = useState(connection?.principal.type ?? 'user');
  const [principalId, setPrincipalId] = useState(connection?.principal.id ?? 'local');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const normalized = useMemo(() => {
    try {
      return normalizeBaseUrl(baseUrl);
    } catch {
      return null;
    }
  }, [baseUrl]);

  async function connect() {
    setError('');
    setSaving(true);
    try {
      const candidateUrl = normalizeBaseUrl(baseUrl);
      const principal = {
        type: principalType.trim() || 'user',
        id: principalId.trim() || 'local',
      };
      const candidate = new HubClient({
        baseUrl: candidateUrl,
        principal,
        accessToken: accessToken.trim() || undefined,
      });
      await candidate.health();
      await candidate.capabilities();
      await candidate.session();
      await save({
        baseUrl: candidateUrl,
        principal,
        accessToken: accessToken.trim() || undefined,
        preserveAccessToken: Boolean(connection && hasAccessToken && !accessToken.trim()),
      });
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
      router.replace('/projects');
    } catch (caught) {
      setError(formatError(caught));
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    } finally {
      setSaving(false);
    }
  }

  return (
    <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Stack.Screen
        options={{
          title: connection ? 'Hub connection' : 'Connect to Hub',
          presentation: connection ? 'formSheet' : 'card',
          sheetGrabberVisible: Boolean(connection),
          sheetAllowedDetents: connection ? [0.72, 1] : undefined,
        }}
      />
      <Page>
        <Content>
          {!connection ? (
            <View style={{ alignItems: 'center', gap: 9, paddingVertical: 10 }}>
              <Image
                source={require('../../assets/icon.png')}
                accessibilityIgnoresInvertColors
                style={{ width: 78, height: 78, borderRadius: 19 }}
              />
              <Eyebrow>Sorrel Hub</Eyebrow>
              <Title>Your work, within reach.</Title>
            </View>
          ) : null}
          <View style={{ gap: 8 }}>
            <Body>
              Connect the native companion to a Sorrel Hub that this device can reach. Use the Hub
              origin directly—do not add /api.
            </Body>
          </View>

          <Field
            label="Hub URL"
            value={baseUrl}
            onChangeText={setBaseUrl}
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            placeholder="https://hub.example.com"
            textContentType="URL"
          />

          {normalized && isInsecureConnection(normalized) ? (
            <Notice tone="warning">
              Plain HTTP is for a trusted local development network only. Use HTTPS for remote or
              production Hubs.
            </Notice>
          ) : null}

          <Field
            label="Bearer token (optional)"
            value={accessToken}
            onChangeText={setAccessToken}
            autoCapitalize="none"
            autoCorrect={false}
            secureTextEntry
            placeholder={hasAccessToken ? 'Saved securely — leave blank to keep' : 'OIDC access token'}
            textContentType="password"
            hint="Stored in iOS Keychain or Android Keystore. Saved credentials are never prefilled."
          />

          <Notice>
            The acting principal below is used only by Hub development auth. OIDC and WorkOS
            sessions resolve identity on the server and ignore it.
          </Notice>

          <View style={{ flexDirection: 'row', gap: 12 }}>
            <View style={{ flex: 1 }}>
              <Field
                label="Principal type"
                value={principalType}
                onChangeText={setPrincipalType}
                autoCapitalize="none"
                autoCorrect={false}
              />
            </View>
            <View style={{ flex: 2 }}>
              <Field
                label="Principal id"
                value={principalId}
                onChangeText={setPrincipalId}
                autoCapitalize="none"
                autoCorrect={false}
              />
            </View>
          </View>

          {error ? <Notice tone="danger">{error}</Notice> : null}
          <Button label={saving ? 'Checking Hub…' : 'Connect'} disabled={saving} onPress={connect} />
        </Content>
      </Page>
    </KeyboardAvoidingView>
  );
}
