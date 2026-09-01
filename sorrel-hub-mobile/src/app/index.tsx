import { Redirect } from 'expo-router';

import { LoadingState } from '@/components/ui';
import { useHub } from '@/context/hub-context';

export default function Index() {
  const { ready, connection } = useHub();
  if (!ready) return <LoadingState label="Opening Sorrel…" />;
  return <Redirect href={connection ? '/projects' : '/connect'} />;
}
