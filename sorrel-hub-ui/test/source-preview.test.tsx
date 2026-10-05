import { render, screen } from '@solidjs/testing-library';
import { expect, test } from 'vitest';
import { SourcePreview } from '../src/components/SourcePreview.tsx';

test('renders source as text without interpreting markup', () => {
  render(() => <SourcePreview content={'<script>alert(1)</script>\nnext'} label="Source" />);
  expect(screen.getByLabelText('Source')).toHaveTextContent('<script>alert(1)</script>');
  expect(document.querySelector('script')).toBeNull();
});
test('does not display binary or excessive file content', () => {
  render(() => <><SourcePreview content={'binary\0payload'} label="Binary" /><SourcePreview content={'x'.repeat(512 * 1024 + 1)} label="Large" /></>);
  expect(screen.getByText('Binary files cannot be previewed as text.')).toBeInTheDocument();
  expect(screen.getByText('This file is too large to preview.')).toBeInTheDocument();
  expect(screen.queryByLabelText('Binary')).toBeNull();
  expect(screen.queryByLabelText('Large')).toBeNull();
});
