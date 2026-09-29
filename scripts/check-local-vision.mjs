/**
 * Release smoke test for the exact Qwen3.5 llama-server contract used by the app.
 * The image is a tiny, synthetic PNG; this checks loading and the multimodal API,
 * not classification accuracy. No user screen, account, or database is read.
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { createServer } from "node:net";

const root = resolve(import.meta.dirname, "..");
const bin = join(root, "local_llm", "bin", process.platform === "win32" ? "llama-server.exe" : "llama-server");
const model = join(root, "local_llm", "Qwen3.5-2B-Q6_K.gguf");
const projector = join(root, "local_llm", "mmproj-Qwen3.5-2B-Q8_0.gguf");
const alias = "flowsight-qwen3.5-2b";
// A public-domain 1x1 PNG. Its content is deliberately not used as an accuracy test.
const image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a+FQAAAAASUVORK5CYII=";

for (const file of [bin, model, projector]) {
  if (!existsSync(file)) throw new Error(`Required local model asset is missing: ${file}`);
}

const port = await new Promise((done, reject) => {
  const server = createServer();
  server.once("error", reject);
  server.listen(0, "127.0.0.1", () => {
    const selected = server.address().port;
    server.close(() => done(selected));
  });
});

const args = [
  "-m", model,
  "--mmproj", projector,
  "--alias", alias,
  "--reasoning-budget", "0",
  "--host", "127.0.0.1",
  "--port", String(port),
  "--ctx-size", "4096",
  "--parallel", "2",
  "--threads", "2",
  "--n-gpu-layers", "0",
];
const child = spawn(bin, args, {
  cwd: join(root, "local_llm", "bin"),
  env: { ...process.env, GGML_DISABLE_VULKAN: "1", LLAMA_ARG_DEVICE: "none" },
  stdio: ["ignore", "pipe", "pipe"],
  windowsHide: true,
});
let tail = "";
for (const stream of [child.stdout, child.stderr]) {
  stream.on("data", (chunk) => {
    tail = (tail + chunk.toString()).slice(-6000);
  });
}

const origin = `http://127.0.0.1:${port}`;
const deadline = Date.now() + 180_000;
try {
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`llama-server exited ${child.exitCode}\n${tail}`);
    try {
      const health = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(3000) });
      if (health.ok && (await health.json()).status === "ok") break;
    } catch { /* still loading */ }
    await new Promise((done) => setTimeout(done, 1000));
  }
  if (Date.now() >= deadline) throw new Error(`llama-server did not become healthy\n${tail}`);

  const response = await fetch(`${origin}/v1/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    signal: AbortSignal.timeout(120_000),
    body: JSON.stringify({
      model: alias,
      messages: [{
        role: "user",
        content: [
          { type: "text", text: "Answer briefly: what is visible in the attached image?" },
          { type: "image_url", image_url: { url: `data:image/png;base64,${image}` } },
        ],
      }],
      temperature: 0,
      max_tokens: 80,
      stream: false,
    }),
  });
  const body = await response.json();
  if (!response.ok) throw new Error(`Multimodal request failed (${response.status}): ${JSON.stringify(body)}`);
  const answer = body?.choices?.[0]?.message?.content?.trim();
  if (body.model !== alias || !answer) {
    throw new Error(`Wrong model alias or empty multimodal content: ${JSON.stringify(body)}`);
  }
  console.log(`[check-local-vision] OK: ${alias}, multimodal response ${answer.length} chars`);
} finally {
  child.kill(); // Only the child launched by this test; never kill an installed FlowSight server.
}
