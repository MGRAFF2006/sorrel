import { createSignal } from 'solid-js';
import type { Principal } from './api.ts';
import { LOCAL_PRINCIPAL } from './api.ts';

const STORAGE_KEY = 'sorrel.hub.actingPrincipal';

function readStoredPrincipal(): Principal {
  if (typeof localStorage === 'undefined') {
    return LOCAL_PRINCIPAL;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return LOCAL_PRINCIPAL;
    const parsed = JSON.parse(raw) as Principal;
    if (
      parsed &&
      typeof parsed === 'object' &&
      typeof parsed.type === 'string' &&
      typeof parsed.id === 'string'
    ) {
      return parsed;
    }
  } catch {
    /* ignore corrupt storage */
  }
  return LOCAL_PRINCIPAL;
}

const [actingPrincipal, setActingPrincipalSignal] = createSignal<Principal>(readStoredPrincipal());

export function getActingPrincipal(): Principal {
  return actingPrincipal();
}

export function useActingPrincipal() {
  return actingPrincipal;
}

export function setActingPrincipal(principal: Principal) {
  setActingPrincipalSignal(principal);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(principal));
  }
}

export const DEV_IDENTITY_PRESETS: Principal[] = [
  LOCAL_PRINCIPAL,
  { type: 'user', id: 'reviewer' },
  { type: 'user', id: 'maintainer' },
  { type: 'agent', id: 'ci' },
];
