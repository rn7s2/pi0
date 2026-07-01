// LLM seam (main process only — keeps any API key out of the renderer).
//
// M1 wires the Vercel `ai` SDK but ships no user-facing LLM feature; this
// `analyze` helper is the single entry point a later milestone can call from an
// IPC handler. Requires `OPENAI_API_KEY` in the environment when actually used.
import { openai } from '@ai-sdk/openai';
import { generateText } from 'ai';

import type { TextRecord } from '../shared/schemas';

/** Summarize a set of keystroke records. Not invoked in M1. */
export async function analyze(records: TextRecord[]): Promise<string> {
    const log = records
        .map((r) => `[${new Date(r.ts).toISOString()} ${r.app}] ${r.text}`)
        .join('\n');
    const { text } = await generateText({
        model: openai('gpt-4o-mini'),
        prompt: `Summarize the following activity log into a few bullet points:\n\n${log}`,
    });
    return text;
}
