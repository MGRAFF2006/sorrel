import * as Haptics from 'expo-haptics';
import type { PropsWithChildren, ReactNode } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  type TextInputProps,
  View,
  type ViewStyle,
} from 'react-native';

import { formatError } from '@/lib/domain';
import { useSorrelTheme } from '@/lib/theme';

export function Page({ children, style }: PropsWithChildren<{ style?: ViewStyle }>) {
  const theme = useSorrelTheme();
  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: theme.colors.background }}
      contentContainerStyle={[styles.page, style]}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
    >
      {children}
    </ScrollView>
  );
}

export function Content({ children }: PropsWithChildren) {
  return <View style={styles.content}>{children}</View>;
}

export function Eyebrow({ children }: PropsWithChildren) {
  const theme = useSorrelTheme();
  return <Text style={[styles.eyebrow, { color: theme.colors.accent }]}>{children}</Text>;
}

export function Title({ children }: PropsWithChildren) {
  const theme = useSorrelTheme();
  return <Text style={[styles.title, { color: theme.colors.text }]}>{children}</Text>;
}

export function Body({ children, muted = false }: PropsWithChildren<{ muted?: boolean }>) {
  const theme = useSorrelTheme();
  return (
    <Text style={[styles.body, { color: muted ? theme.colors.muted : theme.colors.text }]}>
      {children}
    </Text>
  );
}

export function Mono({ children, muted = false }: PropsWithChildren<{ muted?: boolean }>) {
  const theme = useSorrelTheme();
  return (
    <Text
      selectable
      style={[styles.mono, { color: muted ? theme.colors.muted : theme.colors.text }]}
    >
      {children}
    </Text>
  );
}

export function Section({ title, children }: PropsWithChildren<{ title?: string }>) {
  const theme = useSorrelTheme();
  return (
    <View style={styles.section}>
      {title ? <Text style={[styles.sectionTitle, { color: theme.colors.muted }]}>{title}</Text> : null}
      {children}
    </View>
  );
}

export function Card({ children, style }: PropsWithChildren<{ style?: ViewStyle }>) {
  const theme = useSorrelTheme();
  return (
    <View
      style={[
        styles.card,
        { backgroundColor: theme.colors.surface, borderColor: theme.colors.border },
        style,
      ]}
    >
      {children}
    </View>
  );
}

export function ListRow({
  title,
  subtitle,
  detail,
  onPress,
  accessibilityLabel,
}: {
  title: string;
  subtitle?: string;
  detail?: ReactNode;
  onPress?: () => void;
  accessibilityLabel?: string;
}) {
  const theme = useSorrelTheme();
  const content = (
    <>
      <View style={styles.rowCopy}>
        <Text numberOfLines={1} style={[styles.rowTitle, { color: theme.colors.text }]}>
          {title}
        </Text>
        {subtitle ? (
          <Text numberOfLines={2} style={[styles.rowSubtitle, { color: theme.colors.muted }]}>
            {subtitle}
          </Text>
        ) : null}
      </View>
      {detail}
      {onPress ? <Text style={[styles.chevron, { color: theme.colors.muted }]}>›</Text> : null}
    </>
  );
  return onPress ? (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? title}
      onPress={onPress}
      style={({ pressed }) => [
        styles.listRow,
        { backgroundColor: theme.colors.surface, borderColor: theme.colors.border },
        pressed && { opacity: 0.68 },
      ]}
    >
      {content}
    </Pressable>
  ) : (
    <View style={[styles.listRow, { backgroundColor: theme.colors.surface, borderColor: theme.colors.border }]}>
      {content}
    </View>
  );
}

export function StatusPill({ value }: { value?: string }) {
  const theme = useSorrelTheme();
  const normalized = value?.toLowerCase() ?? 'unknown';
  const color = ['active', 'open', 'approved', 'succeeded', 'resolved'].includes(normalized)
    ? theme.colors.success
    : ['rejected', 'failed', 'closed'].includes(normalized)
      ? theme.colors.danger
      : ['draft', 'queued', 'running', 'in_progress'].includes(normalized)
        ? theme.colors.warning
        : theme.colors.muted;
  return (
    <View style={[styles.pill, { borderColor: color }]}>
      <Text style={[styles.pillText, { color }]}>{normalized.replaceAll('_', ' ')}</Text>
    </View>
  );
}

export function Field({ label, hint, ...props }: TextInputProps & { label: string; hint?: string }) {
  const theme = useSorrelTheme();
  return (
    <View style={styles.field}>
      <Text style={[styles.label, { color: theme.colors.text }]}>{label}</Text>
      <TextInput
        {...props}
        accessibilityLabel={props.accessibilityLabel ?? label}
        placeholderTextColor={theme.colors.muted}
        selectionColor={theme.colors.accent}
        style={[
          styles.input,
          props.multiline && styles.multiline,
          { color: theme.colors.text, backgroundColor: theme.colors.input, borderColor: theme.colors.border },
          props.style,
        ]}
      />
      {hint ? <Text style={[styles.hint, { color: theme.colors.muted }]}>{hint}</Text> : null}
    </View>
  );
}

export function Button({
  label,
  onPress,
  disabled,
  kind = 'primary',
}: {
  label: string;
  onPress: () => void;
  disabled?: boolean;
  kind?: 'primary' | 'secondary' | 'danger';
}) {
  const theme = useSorrelTheme();
  const backgroundColor =
    kind === 'primary' ? theme.colors.accent : kind === 'danger' ? theme.colors.danger : theme.colors.surfaceRaised;
  const color = kind === 'primary' || kind === 'danger' ? '#20252E' : theme.colors.text;
  return (
    <Pressable
      accessibilityRole="button"
      disabled={disabled}
      onPress={() => {
        void Haptics.selectionAsync();
        onPress();
      }}
      style={({ pressed }) => [
        styles.button,
        { backgroundColor },
        (disabled || pressed) && { opacity: disabled ? 0.42 : 0.72 },
      ]}
    >
      <Text style={[styles.buttonLabel, { color }]}>{label}</Text>
    </Pressable>
  );
}

export function Notice({ children, tone = 'info' }: PropsWithChildren<{ tone?: 'info' | 'warning' | 'danger' }>) {
  const theme = useSorrelTheme();
  const color = tone === 'danger' ? theme.colors.danger : tone === 'warning' ? theme.colors.warning : theme.colors.accent;
  return (
    <View style={[styles.notice, { backgroundColor: theme.colors.surface, borderColor: color }]}>
      <Text style={[styles.noticeText, { color: theme.colors.text }]}>{children}</Text>
    </View>
  );
}

export function LoadingState({ label = 'Loading…' }: { label?: string }) {
  const theme = useSorrelTheme();
  return (
    <View style={styles.centerState} accessibilityRole="progressbar">
      <ActivityIndicator color={theme.colors.accent} />
      <Text style={[styles.stateBody, { color: theme.colors.muted }]}>{label}</Text>
    </View>
  );
}

export function EmptyState({ title, body, action }: { title: string; body: string; action?: ReactNode }) {
  const theme = useSorrelTheme();
  return (
    <View style={styles.centerState}>
      <Text style={[styles.stateTitle, { color: theme.colors.text }]}>{title}</Text>
      <Text style={[styles.stateBody, { color: theme.colors.muted }]}>{body}</Text>
      {action}
    </View>
  );
}

export function ErrorState({ error, retry }: { error: unknown; retry?: () => void }) {
  return (
    <EmptyState
      title="Couldn’t load this view"
      body={formatError(error)}
      action={retry ? <Button label="Try again" kind="secondary" onPress={retry} /> : undefined}
    />
  );
}

export const styles = StyleSheet.create({
  page: { paddingHorizontal: 16, paddingTop: 14, paddingBottom: 48, gap: 18 },
  content: { width: '100%', maxWidth: 1120, alignSelf: 'center', gap: 16 },
  eyebrow: { fontSize: 12, fontWeight: '800', letterSpacing: 1.1, textTransform: 'uppercase' },
  title: { fontSize: 28, fontWeight: '800', letterSpacing: -0.7 },
  body: { fontSize: 16, lineHeight: 23 },
  mono: { fontSize: 13, lineHeight: 19, fontFamily: 'monospace' },
  section: { gap: 10 },
  sectionTitle: { fontSize: 13, fontWeight: '700', textTransform: 'uppercase', letterSpacing: 0.7, marginLeft: 4 },
  card: { borderWidth: StyleSheet.hairlineWidth, borderRadius: 18, padding: 16, gap: 10 },
  listRow: { minHeight: 76, borderWidth: StyleSheet.hairlineWidth, borderRadius: 16, paddingHorizontal: 16, paddingVertical: 13, flexDirection: 'row', alignItems: 'center', gap: 12 },
  rowCopy: { flex: 1, gap: 4 },
  rowTitle: { fontSize: 17, fontWeight: '700' },
  rowSubtitle: { fontSize: 14, lineHeight: 19 },
  chevron: { fontSize: 30, fontWeight: '300', marginLeft: 2 },
  pill: { borderWidth: 1, borderRadius: 999, paddingHorizontal: 9, paddingVertical: 4, alignSelf: 'center' },
  pillText: { fontSize: 11, fontWeight: '800', textTransform: 'uppercase', letterSpacing: 0.4 },
  field: { gap: 7 },
  label: { fontSize: 14, fontWeight: '700' },
  input: { minHeight: 50, borderWidth: 1, borderRadius: 13, paddingHorizontal: 14, fontSize: 16 },
  multiline: { minHeight: 112, paddingTop: 13, textAlignVertical: 'top' },
  hint: { fontSize: 12, lineHeight: 17 },
  button: { minHeight: 48, borderRadius: 13, paddingHorizontal: 18, justifyContent: 'center', alignItems: 'center' },
  buttonLabel: { fontSize: 16, fontWeight: '800' },
  notice: { borderWidth: 1, borderRadius: 14, padding: 13 },
  noticeText: { fontSize: 14, lineHeight: 20 },
  centerState: { minHeight: 220, padding: 28, justifyContent: 'center', alignItems: 'center', gap: 12 },
  stateTitle: { fontSize: 19, fontWeight: '800', textAlign: 'center' },
  stateBody: { fontSize: 15, lineHeight: 21, textAlign: 'center', maxWidth: 420 },
});
