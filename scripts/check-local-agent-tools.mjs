// Local, read-only model smoke test. Set FLOWSIGHT_LLAMA_RUNTIME to the unpacked
// Qwen3-VL runtime; set FLOWSIGHT_SMOKE_MATRIX=1 to route all tool families.
import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const root = process.cwd();
const runtime = process.env.FLOWSIGHT_LLAMA_RUNTIME;
if (!runtime) throw new Error('Set FLOWSIGHT_LLAMA_RUNTIME to the unpacked local_llm directory.');
const target = process.env.CARGO_TARGET_DIR || join(root, 'apps/agent/src-tauri/target');
const schemas = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--bin', 'dump_tool_schemas'], {
  cwd: join(root, 'apps/agent/src-tauri'),
  env: { ...process.env, CARGO_TARGET_DIR: target },
  encoding: 'utf8',
}));
const tools = schemas.tools;
const disabledWrites = ['messages_send', 'project_update_status', 'project_create_subtask'];
if (tools.length !== 33 || disabledWrites.some((name) => tools.some((tool) => tool.function.name === name))) {
  throw new Error('Unverified external writes were offered to the local model.');
}
const port = await new Promise((resolve) => {
  const server = createServer();
  server.listen(0, '127.0.0.1', () => {
    const chosen = server.address().port;
    server.close(() => resolve(chosen));
  });
});
const child = spawn(join(runtime, 'bin/llama-server.exe'), [
  '-m', join(runtime, 'Qwen3VL-2B-Instruct-Q4_K_M.gguf'),
  '--mmproj', join(runtime, 'mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf'),
  '--alias', 'flowsight-qwen3vl-2b-instruct',
  '--reasoning-budget', '0',
  '--chat-template-kwargs', '{"enable_thinking":false}',
  '--host', '127.0.0.1', '--port', String(port),
  '--ctx-size', '8192', '--parallel', '2', '--threads', '2', '--n-gpu-layers', '0',
], {
  cwd: join(runtime, 'bin'),
  env: { ...process.env, GGML_DISABLE_VULKAN: '1', LLAMA_ARG_DEVICE: 'none' },
  stdio: ['ignore', 'pipe', 'pipe'],
  windowsHide: true,
});
let tail = '';
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', (chunk) => { tail = (tail + chunk.toString()).slice(-5000); });
}
const origin = `http://127.0.0.1:${port}`;
try {
  const deadline = Date.now() + 180_000;
  let ready = false;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`llama-server exited ${child.exitCode}\n${tail}`);
    try {
      const health = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(3000) });
      if (health.ok && (await health.json()).status === 'ok') { ready = true; break; }
    } catch { /* still loading */ }
    await delay(1000);
  }
  if (!ready) throw new Error(`Qwen did not become ready\n${tail}`);
  const routeTool = [schemas.routeTool];
  const routeMessage = schemas.routePrompt;
  async function completion(messages, offered, choice = 'auto', maxTokens = 700) {
    const response = await fetch(`${origin}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        model: 'flowsight-qwen3vl-2b-instruct', messages,
        ...(offered.length ? { tools: offered, tool_choice: choice } : {}),
        temperature: 0, max_tokens: maxTokens, stream: false,
      }),
      signal: AbortSignal.timeout(120_000),
    });
    return { status: response.status, body: await response.json() };
  }
  const routed = await completion([
    { role: 'system', content: routeMessage },
    { role: 'user', content: 'Start a gentle 45-minute focus block to review the pull request.' },
  ], routeTool, 'required', 96);
  const routeCall = routed.body.choices?.[0]?.message?.tool_calls?.[0]?.function;
  const routeFamily = routeCall?.name === 'select_tool_family' ? JSON.parse(routeCall.arguments).family : null;
  console.log(JSON.stringify({ scenario: 'route focus', status: routed.status, routeFamily, promptTokens: routed.body.usage?.prompt_tokens, error: routed.body.error?.message }));
  if (routed.status !== 200 || routeFamily !== 'focus') process.exitCode = 1;
  const focusTools = tools.filter((tool) => tool.function.name.startsWith('focus_'));
  const { status, body } = await completion([
    { role: 'system', content: 'You are FlowSight on-device action assistant. Suggest one tool call for a clear user request; the host will require confirmation before executing it.' },
    { role: 'user', content: 'Start a gentle 45-minute focus block to review the pull request.' },
  ], focusTools, 'auto', 128);
  console.log(JSON.stringify({
    scenario: 'action focus', status,
    toolCount: focusTools.length,
    promptTokens: body.usage?.prompt_tokens,
    completionTokens: body.usage?.completion_tokens,
    toolNames: body.choices?.[0]?.message?.tool_calls?.map((call) => call.function?.name),
    error: body.error?.message,
  }));
  if (status !== 200 || body.choices?.[0]?.message?.tool_calls?.[0]?.function?.name !== 'focus_start') {
    process.exitCode = 1;
  }

  let seed = 123456789;
  const noisyText = (length) => {
    let result = '';
    while (result.length < length) {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      result += seed.toString(36).padStart(7, '0') + ' ';
    }
    return result.slice(0, length);
  };
  const longTitle = `Review the pull request, investigate a failing build, and write a clear update ${noisyText(150)}`;
  const context = {
    now: '2026-09-30T10:00:00+02:00',
    currentIntention: longTitle,
    focus: { status: 'paused', intention: longTitle },
    tasks: Array.from({ length: 5 }, (_, index) => ({ id: `task-${index}`, title: noisyText(120), status: 'open', priority: 2 })),
    preferences: Array.from({ length: 5 }, (_, index) => ({ key: `preference-${index}`, value: noisyText(120) })),
  };
  const longRequest = (`Please help me plan and start a gentle focus block to review the pull request. Extra context: ${noisyText(1200)}`).slice(0, 1200);
  const previous = Array.from({ length: 4 }, (_, index) => ({ role: index % 2 ? 'assistant' : 'user', content: noisyText(350) }));
  const stressRoute = await completion([
    { role: 'system', content: routeMessage },
    ...previous.map((item) => ({ ...item, content: item.content.slice(0, 250) })),
    { role: 'user', content: longRequest },
  ], routeTool, 'required', 96);
  console.log(JSON.stringify({ scenario: 'long route', status: stressRoute.status, promptTokens: stressRoute.body.usage?.prompt_tokens, error: stressRoute.body.error?.message }));
  if (stressRoute.status !== 200) process.exitCode = 1;
  const stressMessages = [
    { role: 'system', content: 'You are FlowSight on-device action assistant. Suggest at most one function call per turn. Use only the explicit user request and available tools. Never claim an action happened before FlowSight confirms its result. Ask a clarification if arguments are missing. Do not send messages or change external calendars without confirmation. All times need an explicit timezone offset.' },
    { role: 'system', content: `Current FlowSight context: ${JSON.stringify(context)}` },
    ...previous,
    { role: 'user', content: longRequest },
  ];
  const { status: stressStatus, body: stressBody } = await completion(stressMessages, focusTools);
  console.log(JSON.stringify({
    scenario: 'bounded maximum context',
    status: stressStatus,
    promptTokens: stressBody.usage?.prompt_tokens,
    completionTokens: stressBody.usage?.completion_tokens,
    toolNames: stressBody.choices?.[0]?.message?.tool_calls?.map((call) => call.function?.name),
    error: stressBody.error?.message,
  }));
  if (stressStatus !== 200) process.exitCode = 1;
  if (process.env.FLOWSIGHT_SMOKE_MATRIX === '1') {
    const cases = [
      ['system', 'Silence Windows notification banners for 30 minutes.'],
      ['browser', 'Block youtube.com/shorts in my paired browser for 30 minutes.'],
      ['tasks', 'Create a local task called Review the release checklist.'],
      ['calendar', 'Check my calendar availability tomorrow from 09:00 to 10:00.'],
      ['notifications', 'Show the reminders FlowSight held during my focus block.'],
      ['messages', 'Draft an email to alex@example.com saying I am focusing until noon.'],
      ['project', 'Show my current GitHub, Jira and Linear work items.'],
      ['desktop', 'Open my local project folder C:/Projects/FlowSight.'],
      ['automation', 'Run the end-of-day playbook to close my focus session and plan tomorrow.'],
      ['memory', 'Remember that this YouTube channel is research, not a distraction.'],
      ['chat', 'What is the difference between deep work and ordinary work?'],
      ['system', 'Activa No Molestar en Windows durante 30 minutos.'],
      ['messages', 'Redacta un email a alex@example.com diciendo que estaré concentrado hasta las 12.'],
      ['automation', 'Ejecuta la rutina de cierre del día y termina mi bloque de foco.'],
      ['memory', 'Recuerda que este canal de YouTube es investigación, no una distracción.'],
      ['desktop', 'Abre la carpeta local C:/Projects/FlowSight.'],
      ['focus', 'Empieza un bloque de foco de 45 minutos para cerrar el PR.'],
    ];
    for (const [expected, message] of cases) {
      const response = await completion([
        { role: 'system', content: routeMessage },
        { role: 'user', content: message },
      ], routeTool, 'required', 96);
      const call = response.body.choices?.[0]?.message?.tool_calls?.[0]?.function;
      let actual = null;
      try { actual = call?.name === 'select_tool_family' ? JSON.parse(call.arguments).family : null; } catch { /* bad model output */ }
      console.log(JSON.stringify({ scenario: 'family route', expected, actual, status: response.status }));
      if (response.status !== 200 || actual !== expected) process.exitCode = 1;
    }
  }
} finally {
  child.kill();
}
