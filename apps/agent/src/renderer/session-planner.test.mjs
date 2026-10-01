import test from 'node:test';
import assert from 'node:assert/strict';
import { sessionWindow } from './session-planner.mjs';
test('local availability reaches the host with explicit timezone and duration', () => {
  const r = sessionWindow('09:00', '11:00', new Date(2026, 9, 1, 8, 0));
  assert.equal(Date.parse(r.endAt) - Date.parse(r.startAt), 7200000);
  assert.equal(new Date(r.startAt).getHours(), 9); assert.match(r.startAt, /[+-]\d{2}:\d{2}$/);
});
test('rejects elapsed, inverted, overnight, oversized and malformed availability', () => {
  const now = new Date(2026, 9, 1, 8, 0);
  for (const [a,b] of [['07:00','11:00'],['12:00','11:00'],['23:00','01:00'],['08:00','08:10'],['29:00','11:00'],['9:00','11:00']]) assert.throws(() => sessionWindow(a,b,now));
  assert.throws(() => sessionWindow('00:00','23:00',new Date(2026,9,1,0,0)));
});
