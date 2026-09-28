import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const executable = process.argv[2];
if (!executable) {
  throw new Error('Usage: node scripts/check-mcp-stdio.mjs <FlowSight executable>');
}

const requests = [
  { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18' } },
  { jsonrpc: '2.0', method: 'notifications/initialized' },
  { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
  { jsonrpc: '2.0', id: 3, method: 'tools/call', params: {
    name: 'generate_work_report',
    arguments: { period_days: 31 },
  } },
  { jsonrpc: '2.0', id: 4, method: 'tools/call', params: {
    name: 'generate_work_report',
    arguments: { period_days: 7 },
  } },
];

const child = spawnSync(executable, ['--mcp'], {
  input: requests.map((request) => JSON.stringify(request)).join('\n') + '\n',
  encoding: 'utf8',
  timeout: 20_000,
  windowsHide: true,
  maxBuffer: 10 * 1024 * 1024,
});

if (child.error) throw child.error;
assert.equal(child.status, 0, child.stderr);
const lines = child.stdout.trim().split(/\r?\n/);
assert.equal(lines.length, 4, 'MCP mode must write only JSON-RPC responses to stdout');
const responses = lines.map((line) => JSON.parse(line));
assert.deepEqual(responses.map((response) => response.id), [1, 2, 3, 4]);
assert.equal(responses[0].result.protocolVersion, '2025-06-18');
assert.equal(responses[1].result.tools[0].name, 'generate_work_report');
assert.equal(responses[2].result.isError, true);
assert.ok(
  responses[3].result.structuredContent?.report_version === 1 ||
  responses[3].result.isError === true,
  'A valid request must return a report or a clear missing-database error'
);
console.log('FlowSight MCP STDIO smoke test passed');
