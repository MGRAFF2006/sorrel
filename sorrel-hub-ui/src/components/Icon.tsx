import type { JSX } from 'solid-js';

export type IconName =
  | 'archive' | 'book' | 'branch' | 'check' | 'chevron' | 'code' | 'folder'
  | 'inbox' | 'layers' | 'more' | 'org' | 'project' | 'refresh' | 'repo'
  | 'search' | 'sync' | 'user' | 'users' | 'workflow';

const paths: Record<IconName, JSX.Element> = {
  archive: <><path d="M4 7h16v13H4z"/><path d="M3 4h18v3H3zM9 11h6"/></>,
  book: <><path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v16H6.5A2.5 2.5 0 0 0 4 21.5z"/><path d="M4 5.5v16M8 7h8"/></>,
  branch: <><circle cx="6" cy="5" r="2"/><circle cx="18" cy="7" r="2"/><circle cx="6" cy="19" r="2"/><path d="M6 7v10M8 9c5 0 5-2 8-2"/></>,
  check: <path d="m5 12 4 4L19 6"/>,
  chevron: <path d="m9 18 6-6-6-6"/>,
  code: <><path d="m8 9-3 3 3 3m8-6 3 3-3 3M14 5l-4 14"/></>,
  folder: <path d="M3 6h7l2 2h9v11H3z"/>,
  inbox: <><path d="M4 5h16l1.5 10.5A3 3 0 0 1 18.5 19h-13a3 3 0 0 1-3-3.5L4 5Z"/><path d="M3 14h5l2 2h4l2-2h5"/></>,
  layers: <><path d="m12 3 9 5-9 5-9-5z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/></>,
  more: <><circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/></>,
  org: <><rect x="4" y="3" width="11" height="18"/><path d="M8 7h3m-3 4h3m-3 4h3m4-6h5v12h-5"/></>,
  project: <><rect x="3" y="4" width="18" height="16"/><path d="M3 9h18M8 4v5"/></>,
  refresh: <><path d="M20 7v5h-5"/><path d="M18.5 16a8 8 0 1 1 .2-8.2L20 12"/></>,
  repo: <><path d="M5 3h12a2 2 0 0 1 2 2v16H7a3 3 0 0 1-3-3V4a1 1 0 0 1 1-1Z"/><path d="M7 17h12M8 7h7"/></>,
  search: <><circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/></>,
  sync: <><path d="M20 7h-5V2"/><path d="M4 17h5v5M19 12a7 7 0 0 0-12-5L5 9m0 3a7 7 0 0 0 12 5l2-2"/></>,
  user: <><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></>,
  users: <><circle cx="9" cy="8" r="3"/><path d="M3 20a6 6 0 0 1 12 0m1-15a3 3 0 0 1 0 6m1 3a5 5 0 0 1 4 5"/></>,
  workflow: <><rect x="3" y="3" width="6" height="6"/><rect x="15" y="15" width="6" height="6"/><path d="M9 6h4a5 5 0 0 1 5 5v4M6 9v6a3 3 0 0 0 3 3h6"/></>,
};

export function Icon(props: { name: IconName; class?: string }) {
  return (
    <svg class={`icon ${props.class ?? ''}`} viewBox="0 0 24 24" aria-hidden="true">
      {paths[props.name]}
    </svg>
  );
}
