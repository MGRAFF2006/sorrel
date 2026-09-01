import { For, Show } from 'solid-js';

type Block = { kind: 'h1' | 'h2' | 'h3' | 'quote' | 'bullet' | 'code' | 'paragraph'; text: string };

function parseMarkdown(source: string): Block[] {
  const blocks: Block[] = [];
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  let inCode = false;
  let code = '';

  for (const line of lines) {
    if (line.startsWith('```')) {
      if (inCode) {
        blocks.push({ kind: 'code', text: code.replace(/\n$/, '') });
        code = '';
      }
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      code += `${line}\n`;
      continue;
    }
    if (!line.trim()) continue;
    if (line.startsWith('### ')) blocks.push({ kind: 'h3', text: line.slice(4) });
    else if (line.startsWith('## ')) blocks.push({ kind: 'h2', text: line.slice(3) });
    else if (line.startsWith('# ')) blocks.push({ kind: 'h1', text: line.slice(2) });
    else if (line.startsWith('> ')) blocks.push({ kind: 'quote', text: line.slice(2) });
    else if (/^[-*] /.test(line)) blocks.push({ kind: 'bullet', text: line.slice(2) });
    else blocks.push({ kind: 'paragraph', text: line });
  }
  if (code) blocks.push({ kind: 'code', text: code.replace(/\n$/, '') });
  return blocks;
}

export function MarkdownDocument(props: { source: string; empty?: string }) {
  const blocks = () => parseMarkdown(props.source);
  return (
    <div class="markdown-document">
      <Show when={blocks().length > 0} fallback={<p class="muted">{props.empty ?? 'Nothing written yet.'}</p>}>
        <For each={blocks()}>{(block) => {
          if (block.kind === 'h1') return <h1>{block.text}</h1>;
          if (block.kind === 'h2') return <h2>{block.text}</h2>;
          if (block.kind === 'h3') return <h3>{block.text}</h3>;
          if (block.kind === 'quote') return <blockquote>{block.text}</blockquote>;
          if (block.kind === 'bullet') return <div class="markdown-bullet"><span>—</span><p>{block.text}</p></div>;
          if (block.kind === 'code') return <pre><code>{block.text}</code></pre>;
          return <p>{block.text}</p>;
        }}</For>
      </Show>
    </div>
  );
}
