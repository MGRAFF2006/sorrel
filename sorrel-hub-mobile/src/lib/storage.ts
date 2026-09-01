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
  await SecureStore.setItemAsync(CONNECTION_KEY, JSON.stringify(connection));
  if (options.accessToken) {
    await SecureStore.setItemAsync(ACCESS_TOKEN_KEY, options.accessToken);
    return options.accessToken;
  }
  if (!options.preserveAccessToken) {
    await SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY);
    return undefined;
  }
  return (await SecureStore.getItemAsync(ACCESS_TOKEN_KEY)) ?? undefined;
}

export async function clearConnection(): Promise<void> {
  await Promise.all([
    SecureStore.deleteItemAsync(CONNECTION_KEY),
    SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
  ]);
}
