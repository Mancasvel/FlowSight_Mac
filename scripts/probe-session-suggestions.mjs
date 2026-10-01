// Run the actual extracted planner/model request on synthetic data only.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const runtime = resolve(process.env.FLOWSIGHT_LLAMA_RUNTIME || 'local_llm');
const binarySuffix = process.platform === 'win32' ? '.exe' : '';
const output = resolve('.impeccable/review');
await mkdir(output, { recursive: true });
const port = await new Promise(resolvePort => {
  const listener = createServer();
  listener.listen(0, '127.0.0.1', () => { const value = listener.address().port; listener.close(() => resolvePort(value)); });
});
const server = spawn(join(runtime, `bin/llama-server${binarySuffix}`), [
  '-m', join(runtime, 'Qwen3VL-2B-Instruct-Q4_K_M.gguf'), '--mmproj', join(runtime, 'mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf'),
  '--alias', 'flowsight-qwen3vl-2b-instruct', '--reasoning-budget', '0', '--chat-template-kwargs', '{"enable_thinking":false}',
  '--host', '127.0.0.1', '--port', String(port), '--ctx-size', '8192', '--parallel', '2', '--threads', '2', '--n-gpu-layers', '0',
], { cwd: join(runtime, 'bin'), windowsHide: true, env: { ...process.env, GGML_DISABLE_VULKAN: '1', LLAMA_ARG_DEVICE: 'none' }, stdio: ['ignore', 'pipe', 'pipe'] });
let tail = '';
for (const stream of [server.stdout, server.stderr]) stream.on('data', data => { tail = (tail + data).slice(-6000); });
const request = { intention: 'I want to do 4 exercises of ADDA related with virtual graphs, genetic algorithms, recursive types and PLE', startAt: '2026-10-01T10:35:00+02:00', endAt: '2026-10-01T16:35:00+02:00' };
const run = input => new Promise((resolveResult, reject) => {
  const test = spawn(resolve(`.impeccable/review/suggestions-harness/target/release/flowsight-suggestions-harness${binarySuffix}`), [], {
    windowsHide: true, env: { ...process.env, FLOWSIGHT_PLAN_SMOKE_URL: `http://127.0.0.1:${port}/v1/chat/completions` }, stdio: ['pipe', 'pipe', 'pipe'],
  });
  let stdout = '', stderr = '';
  test.stdout.on('data', chunk => { stdout += chunk; }); test.stderr.on('data', chunk => { stderr += chunk; });
  test.once('error', reject); test.once('exit', code => {
    if (code) reject(new Error(`Harness failed ${code}: ${stderr}`)); else resolveResult(JSON.parse(stdout));
  });
  test.stdin.end(JSON.stringify(input) + '\n');
});
try {
  const deadline = Date.now() + 180000;
  let ready = false;
  while (Date.now() < deadline) {
    if (server.exitCode !== null) throw new Error(`Server exited: ${tail}`);
    try { const response = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(2000) }); if (response.ok && (await response.json()).status === 'ok') { ready = true; break; } } catch {}
    await delay(1000);
  }
  if (!ready) throw new Error(`Model did not load: ${tail}`);
  const started = Date.now();
  const first = await run({ request, events: [] });
  console.log(JSON.stringify({ first }, null, 2));
  const results = { first, elapsedSeconds: (Date.now() - started) / 1000 };
  if (!first.accepted) throw new Error(`The ADDA draft was rejected: ${first.error}`);
  const topics = ['virtual graphs', 'genetic algorithms', 'recursive types', 'PLE'];
  const verify = (result, rest) => {
    if (!result.accepted || !result.calendarUntouched) throw new Error('Draft must be valid and must not write events.');
    const blocks = result.proposal.blocks;
    if (blocks.length !== 7 || result.proposal.unscheduled.length) throw new Error('All four exercises must fit, with three breaks.');
    for (const topic of topics) if (!blocks.some(block => block.title.toLowerCase() === topic.toLowerCase())) throw new Error(`Missing ${topic}`);
    for (let index = 1; index < blocks.length; index += 2) {
      if (blocks[index].title !== 'Break' || Date.parse(blocks[index].endAt) - Date.parse(blocks[index].startAt) !== rest * 60000) throw new Error('Wrong break duration.');
    }
    if (blocks[0].startAt !== request.startAt || Date.parse(blocks.at(-1).endAt) > Date.parse(request.endAt)) throw new Error('Wrong availability.');
  };
  verify(first, 10);
  if (first.accepted && process.env.FLOWSIGHT_PLAN_REVISE === '1') {
    results.revised = await run({ request, previous: first.proposal, feedback: 'Do PLE first and leave a 15-minute break between tasks.', events: [] });
    verify(results.revised, 15);
    if (results.revised.proposal.blocks[0].title !== 'PLE') throw new Error('The revision must put PLE first.');
    console.log(JSON.stringify({ revised: results.revised }, null, 2));
  }
  await writeFile(join(output, process.env.FLOWSIGHT_PLAN_OUTPUT || 'suggestions-qwen.json'), JSON.stringify(results, null, 2));
} finally { server.kill(); }
