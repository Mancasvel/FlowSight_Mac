// Use the shipped Qwen model and the Rust planner's exact request/validator.
// This smoke test supplies synthetic context and never opens the user's database.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const runtime = process.env.FLOWSIGHT_LLAMA_RUNTIME;
if (!runtime) throw new Error('Set FLOWSIGHT_LLAMA_RUNTIME to an unpacked Qwen3-VL local_llm directory.');
const port = await new Promise((resolve) => {
  const listener = createServer();
  listener.listen(0, '127.0.0.1', () => {
    const selected = listener.address().port;
    listener.close(() => resolve(selected));
  });
});
const child = spawn(join(runtime, 'bin/llama-server.exe'), [
  '-m', join(runtime, 'Qwen3VL-2B-Instruct-Q4_K_M.gguf'),
  '--mmproj', join(runtime, 'mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf'),
  '--alias', 'flowsight-qwen3vl-2b-instruct', '--reasoning-budget', '0',
  '--chat-template-kwargs', '{"enable_thinking":false}',
  '--host', '127.0.0.1', '--port', String(port),
  '--ctx-size', '8192', '--parallel', '2', '--threads', '2', '--n-gpu-layers', '0',
], {
  cwd: join(runtime, 'bin'), windowsHide: true,
  env: { ...process.env, GGML_DISABLE_VULKAN: '1', LLAMA_ARG_DEVICE: 'none' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let tail = '';
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', chunk => { tail = (tail + chunk.toString()).slice(-5000); });
}
try {
  const origin = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + 180_000;
  let ready = false;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Local Qwen exited ${child.exitCode}: ${tail}`);
    try {
      const response = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(3000) });
      if (response.ok && (await response.json()).status === 'ok') { ready = true; break; }
    } catch { /* wait for local loading */ }
    await delay(1000);
  }
  if (!ready) throw new Error(`Local Qwen did not become ready: ${tail}`);
  const code = await new Promise((resolve, reject) => {
    const test = spawn('cargo', ['test', '--manifest-path', 'apps/agent/src-tauri/Cargo.toml', '--lib',
      'local_agent::session_plan::tests::real_qwen_plans_and_revises_without_writing', '--', '--ignored', '--nocapture'], {
      windowsHide: true, stdio: 'inherit',
      env: { ...process.env, FLOWSIGHT_PLAN_SMOKE_URL: `${origin}/v1/chat/completions` },
    });
    test.once('error', reject); test.once('exit', resolve);
  });
  if (code !== 0) throw new Error(`Local session planning failed (${code}). ${tail}`);
} finally {
  child.kill();
}
