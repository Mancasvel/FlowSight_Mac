/**
 * Ensures vision weights live under local_llm/ before `tauri build`.
 * Solo Node (built-in fetch; sin dependencias npm). Equivalente al antiguo
 * scripts/fetch-models.py: mismo release GitHub y flags --force /--check.
 * El usuario instalado desde el .exe NO ejecuta este script.
 *
 * Env: FLOWSIGHT_MODELS_REPO, FLOWSIGHT_MODELS_TAG,
 *      GITHUB_TOKEN | GH_TOKEN, or `gh auth token`.
 *
 * Flags: --force  re-download even if OK
 *        --check  only verify, exit 1 if missing (no net)
 */

import {
  existsSync,
  mkdirSync,
  renameSync,
  statSync,
  unlinkSync,
  createWriteStream,
} from "node:fs";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { finished } from "node:stream/promises";

const DEFAULT_REPO = "Mancasvel/FlowSight.AI";
const DEFAULT_TAG = "models-v0.1.0";
const MIN_SIZE_BYTES = 1_000_000;

/** @type {Record<string, string>} relative paths from repo root */
const ASSETS = {
  "Qwen3-VL-2B-Instruct-Q3_K_M.gguf": "local_llm/Qwen3-VL-2B-Instruct-Q3_K_M.gguf",
  "mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf": "local_llm/mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf",
};

const __dirname = dirname(fileURLToPath(import.meta.url));

function repoRoot() {
  let dir = join(__dirname, "..");
  for (let i = 0; i < 10; i++) {
    if (existsSync(join(dir, ".git"))) return dir;
    const next = dirname(dir);
    if (next === dir) break;
    dir = next;
  }
  return join(__dirname, "..");
}

function resolveGithubToken() {
  for (const key of ["GITHUB_TOKEN", "GH_TOKEN"]) {
    const v = process.env[key]?.trim();
    if (v) return v;
  }
  try {
    const r = spawnSync("gh", ["auth", "token"], {
      encoding: "utf8",
      timeout: 10_000,
    });
    if (r.status === 0 && r.stdout?.trim()) return r.stdout.trim();
  } catch {
    /* no gh */
  }
  return null;
}

function humanSize(n) {
  let val = Number(n);
  let u = 0;
  const units = ["B", "KB", "MB", "GB"];
  while (val >= 1024 && u < units.length - 1) {
    val /= 1024;
    u++;
  }
  return `${val.toFixed(1)} ${units[u]}`;
}

function assetUrl(repo, tag, name) {
  return `https://github.com/${repo}/releases/download/${tag}/${name}`;
}

/** @param {string} path */
function fileOk(path) {
  try {
    return existsSync(path) && statSync(path).size >= MIN_SIZE_BYTES;
  } catch {
    return false;
  }
}

/** @param {string} tmp */
function finalizePart(tmp, dest) {
  if (!existsSync(tmp) || statSync(tmp).size < MIN_SIZE_BYTES) {
    try {
      unlinkSync(tmp);
    } catch {}
    throw new Error(
      `Downloaded file is suspiciously small (< ${humanSize(MIN_SIZE_BYTES)}). ` +
        "Probably an HTML error page. Check tag and permissions.",
    );
  }
  renameSync(tmp, dest);
}

/**
 * @param {string} url
 * @param {string} dest
 * @param {string | null} token
 */
async function download(url, dest, token) {
  mkdirSync(dirname(dest), { recursive: true });
  const tmp = `${dest}.part`;

  try {
    unlinkSync(tmp);
  } catch {
    /* */
  }

  const headers = {
    Accept: "application/octet-stream",
    "User-Agent": "flowsight-fetch-models",
  };
  if (token) headers.Authorization = `Bearer ${token}`;

  try {
    const res = await fetch(url, {
      redirect: "follow",
      headers,
    });

    if (!res.ok) {
      const err = /** @type {any} */ ({
        httpCode: res.status,
        httpText: res.statusText,
        message: `${res.status} ${res.statusText}`,
      });
      throw err;
    }

    const total = +(res.headers.get("content-length") || 0);
    const body = res.body;
    if (!body) throw new Error("Empty response body");

    const fh = createWriteStream(tmp);
    const reader = body.getReader();

    let downloaded = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value?.length) continue;
      fh.write(Buffer.from(value));
      downloaded += value.length;
      if (total > 0) {
        const pct = (downloaded * 100) / total;
        process.stdout.write(
          `  ${humanSize(downloaded)} / ${humanSize(total)} (${pct.toFixed(1)}%)\r`,
        );
      } else {
        process.stdout.write(`  ${humanSize(downloaded)}\r`);
      }
    }

    fh.end();
    await finished(fh);
    process.stdout.write("\n");

    finalizePart(tmp, dest);
  } catch (e) {
    try {
      unlinkSync(tmp);
    } catch {}
    throw e;
  }
}

async function main() {
  const force = process.argv.includes("--force");
  const check = process.argv.includes("--check");

  const repo = process.env.FLOWSIGHT_MODELS_REPO || DEFAULT_REPO;
  const tag = process.env.FLOWSIGHT_MODELS_TAG || DEFAULT_TAG;
  const root = repoRoot();

  console.log(`[fetch-models] repo=${repo} tag=${tag}`);
  console.log(`[fetch-models] root=${root}`);

  /** @type {{ name: string, dest: string }[]} */
  const missing = [];
  for (const [name, rel] of Object.entries(ASSETS)) {
    const dest = join(root, rel);
    if (force || !fileOk(dest)) {
      missing.push({ name, dest });
    } else {
      console.log(`  OK    ${rel} (${humanSize(statSync(dest).size)})`);
    }
  }

  if (missing.length === 0) {
    console.log("[fetch-models] all assets present.");
    return 0;
  }

  if (check) {
    console.error("[fetch-models] MISSING (--check):");
    for (const { dest } of missing) console.error(`  -- ${dest}`);
    return 1;
  }

  const token = resolveGithubToken();
  if (token) console.log("[fetch-models] using auth token from env/gh");

  for (const { name, dest } of missing) {
    const url = assetUrl(repo, tag, name);
    console.log(`[fetch-models] downloading ${name}`);
    console.log(`  from ${url}`);
    console.log(`  to   ${dest}`);

    try {
      await download(url, dest, token);
    } catch (e) {
      const code = /** @type {any} */ (e).httpCode ?? null;
      if (code === 404) {
        console.error(`  Release '${tag}' o asset '${name}' no existe en ${repo}.`);
        console.error(`  gh release upload ${tag} local_llm/${name} …`);
      } else if (code === 401 || code === 403) {
        console.error(
          "  Auth falló. Release privado: exportá GITHUB_TOKEN o `gh auth login`.",
        );
      }
      console.error(`  ERROR: ${e}`);
      return 2;
    }
  }

  console.log("[fetch-models] done.");
  return 0;
}

main().then(
  (code) => process.exit(code),
  (e) => {
    console.error(e);
    process.exit(2);
  },
);
