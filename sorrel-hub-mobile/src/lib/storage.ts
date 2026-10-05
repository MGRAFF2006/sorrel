import * as SecureStore from 'expo-secure-store';

import type { Connection } from './types';

const CONNECTION_KEY = 'sorrel.hub.mobile.connection.v1';
const ACCESS_TOKEN_KEY = 'sorrel.hub.mobile.access-token.v1';

export async function loadConnection(): Promise<{
  connection: Connection | null;
  accessToken?: string;
}> {
  const [rawConnection, accessToken] = await Promise.all([
    SecureStore.getItemAsync(CONNECTION_KEY),
    SecureStore.getItemAsync(ACCESS_TOKEN_KEY),
  ]);
  if (!rawConnection) {
    if (accessToken) await SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY);
    return { connection: null };
  }

  try {
    const candidate = JSON.parse(rawConnection) as Connection;
    if (
      typeof candidate.baseUrl === 'string' &&
      typeof candidate.principal?.type === 'string' &&
      typeof candidate.principal.id === 'string'
    ) {
      return { connection: candidate, accessToken: accessToken ?? undefined };
    }
  } catch {
    // Corrupt local preferences are removed below and never sent to a Hub.
  }
  await Promise.all([
    SecureStore.deleteItemAsync(CONNECTION_KEY),
    SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
  ]);
  return { connection: null };
}

export async function saveConnection(
  connection: Connection,
  options: { accessToken?: string; preserveAccessToken?: boolean } = {},
): Promise<string | undefined> {
  const accessToken = await connectionAccessToken(connection.baseUrl, options);
  // Remove the old endpoint before changing credentials so partial writes cannot
  // associate a token with another Hub after an app restart.
  await SecureStore.deleteItemAsync(CONNECTION_KEY);
  if (accessToken) {
    await SecureStore.setItemAsync(ACCESS_TOKEN_KEY, accessToken);
  } else {
    await SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY);
  }
  await SecureStore.setItemAsync(CONNECTION_KEY, JSON.stringify(connection));
  return accessToken;
}

export async function connectionAccessToken(
  baseUrl: string,
  options: { accessToken?: string; preserveAccessToken?: boolean } = {},
): Promise<string | undefined> {
  if (options.accessToken) return options.accessToken;
  if (!options.preserveAccessToken) return undefined;
  const saved = await loadConnection();
  return saved.connection?.baseUrl === baseUrl ? saved.accessToken : undefined;
}

export async function clearConnection(): Promise<void> {
  await Promise.all([
    SecureStore.deleteItemAsync(CONNECTION_KEY),
    SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
  ]);
}
