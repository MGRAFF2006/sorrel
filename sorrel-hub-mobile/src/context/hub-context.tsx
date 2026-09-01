import { HubClient } from '@sorrel/sdk-js';
import { createContext, type PropsWithChildren, useContext, useEffect, useMemo, useState } from 'react';

import { normalizeBaseUrl } from '@/lib/domain';
import {
  clearConnection as clearStoredConnection,
  loadConnection,
  saveConnection as saveStoredConnection,
} from '@/lib/storage';
import type { Connection, Principal } from '@/lib/types';

type SaveInput = {
  baseUrl: string;
  principal: Principal;
  accessToken?: string;
  preserveAccessToken?: boolean;
};

type HubContextValue = {
  ready: boolean;
  connection: Connection | null;
  client: HubClient | null;
  hasAccessToken: boolean;
  save: (input: SaveInput) => Promise<void>;
  disconnect: () => Promise<void>;
};

const HubContext = createContext<HubContextValue | null>(null);

export function HubProvider({ children }: PropsWithChildren) {
  const [ready, setReady] = useState(false);
  const [connection, setConnection] = useState<Connection | null>(null);
  const [accessToken, setAccessToken] = useState<string | undefined>();

  useEffect(() => {
    let active = true;
    void loadConnection()
      .then((saved) => {
        if (!active) return;
        setConnection(saved.connection);
        setAccessToken(saved.accessToken);
      })
      .catch(() => {
        // A locked or unavailable platform keystore behaves like no saved connection.
      })
      .finally(() => {
        if (active) setReady(true);
      });
    return () => {
      active = false;
    };
  }, []);

  const client = useMemo(
    () =>
      connection
        ? new HubClient({
            baseUrl: connection.baseUrl,
            principal: connection.principal,
            accessToken,
          })
        : null,
    [accessToken, connection],
  );

  async function save(input: SaveInput) {
    const next: Connection = {
      baseUrl: normalizeBaseUrl(input.baseUrl),
      principal: input.principal,
    };
    const nextToken = await saveStoredConnection(next, {
      accessToken: input.accessToken?.trim() || undefined,
      preserveAccessToken: input.preserveAccessToken,
    });
    setAccessToken(nextToken);
    setConnection(next);
  }

  async function disconnect() {
    await clearStoredConnection();
    setAccessToken(undefined);
    setConnection(null);
  }

  return (
    <HubContext.Provider
      value={{
        ready,
        connection,
        client,
        hasAccessToken: Boolean(accessToken),
        save,
        disconnect,
      }}
    >
      {children}
    </HubContext.Provider>
  );
}

export function useHub() {
  const context = useContext(HubContext);
  if (!context) throw new Error('useHub must be used inside HubProvider');
  return context;
}
