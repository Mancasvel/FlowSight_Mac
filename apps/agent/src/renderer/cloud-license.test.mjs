import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { hasPaidCalendarEntitlement } from './calendar-license.mjs';

const markup = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const entitlements = readFileSync(new URL('../../src-tauri/src/entitlements.rs', import.meta.url), 'utf8');
const commands = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8');

test('an integration session is not treated as a FlowSight Cloud account', () => {
  assert.match(markup, /function hasCloudAccountSession\(\)/);
  assert.match(markup, /\['cloud', 'worker', 'google', 'manual'\]\.includes\(authSession\?\.provider\)/);
  assert.match(markup, /if \(session && \['google', 'manual'\]\.includes\(session\.provider\)\)/);
});

test('renderer cannot supply paid entitlements to the native feature gate', () => {
  assert.doesNotMatch(markup, /save_entitlements_command/);
  assert.doesNotMatch(entitlements, /pub fn save_entitlements_command/);
  assert.doesNotMatch(commands, /entitlements::save_entitlements_command/);
  assert.match(markup, /currentEntitlements = await invoke\('get_entitlements'\)/);
  assert.match(entitlements, /refresh_entitlements_from_supabase\(&session\.access_token,\s*&session\.user_id\)/);
});

test('Calendar unlocks only for the signed-in owner of an active Cloud integrations plan', () => {
  const paid = { owner_user_id: 'owner-a', plan: 'individual_pro', status: 'active', can_integrations: true };
  assert.equal(hasPaidCalendarEntitlement(paid, 'owner-a'), true);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, plan: 'individual' }, 'owner-a'), true);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, plan: 'pro' }, 'owner-a'), true);
  assert.equal(hasPaidCalendarEntitlement(paid, 'owner-b'), false);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, owner_user_id: null }, 'owner-a'), false);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, plan: null }, 'owner-a'), false);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, plan: 'team' }, 'owner-a'), false);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, status: 'past_due' }, 'owner-a'), false);
  assert.equal(hasPaidCalendarEntitlement({ ...paid, can_integrations: false }, 'owner-a'), false);
});

test('an active one-time local purchase does not unlock Cloud Calendar', () => {
  const localPurchase = { status: 'active', platform: 'windows' };
  assert.equal(hasPaidCalendarEntitlement(localPurchase, 'owner-a'), false);
  assert.equal(hasPaidCalendarEntitlement({ owner_user_id: 'owner-a', status: 'active', plan: 'individual_local', can_integrations: true }, 'owner-a'), false);
  assert.match(markup, /The one-time local purchase does not include Cloud integrations/);
});
