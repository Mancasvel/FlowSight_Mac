import { createClient } from '@supabase/supabase-js';

const DEFAULT_SUPABASE_URL = 'https://dzpyrdxelcgfpmcdojvb.supabase.co';
const DEFAULT_SUPABASE_ANON_KEY = 'sb_publishable_Ky02yQS5HHpkmrN1DE2yaw_EwENlsPZ';

const supabaseUrl = import.meta.env.NEXT_PUBLIC_SUPABASE_URL || DEFAULT_SUPABASE_URL;
const supabaseAnonKey = import.meta.env.NEXT_PUBLIC_SUPABASE_ANON_KEY || DEFAULT_SUPABASE_ANON_KEY;

let supabaseClient = null;

export function getSupabaseClient() {
  if (!supabaseUrl || !supabaseAnonKey) {
    throw new Error('Supabase public configuration is missing.');
  }

  if (!supabaseClient) {
    supabaseClient = createClient(supabaseUrl, supabaseAnonKey, {
      auth: {
        persistSession: true,
        autoRefreshToken: true,
        detectSessionInUrl: false,
      },
    });
  }

  return supabaseClient;
}

export function getFriendlyAuthError(error) {
  const message = String(error?.message || error || '').toLowerCase();

  if (message.includes('invalid login credentials')) {
    return 'Invalid email or password. Check your license credentials.';
  }

  if (message.includes('email not confirmed')) {
    return 'Please confirm your email before signing in.';
  }

  if (message.includes('no active individual subscription')) {
    return 'This account does not have an active Individual license.';
  }

  if (message.includes('no active') && message.includes('subscription')) {
    return 'No active Individual or Team license found for this account.';
  }

  if (message.includes('supabase public configuration')) {
    return 'Cloud login is not configured yet. Contact support.';
  }

  return 'Login failed. Please try again or contact support.';
}

async function rejectSignedInUser(supabase, message) {
  await supabase.auth.signOut();
  throw new Error(message);
}

function parseEntitlements(raw) {
  const features = raw?.features ?? {};
  const teamIds = Array.isArray(raw?.team_ids)
    ? raw.team_ids.map((id) => String(id))
    : [];

  return {
    plan: raw?.plan ?? null,
    status: raw?.status ?? 'free',
    teamIds,
    activeTeamId: teamIds[0] ?? null,
    canSync: Boolean(features.sync),
    canCloudAi: Boolean(features.cloud_ai),
    canIntegrations: Boolean(features.integrations),
  };
}

export async function fetchUserEntitlements(supabase) {
  const { data, error } = await supabase.rpc('get_user_entitlements');
  if (error) {
    throw new Error(error.message || 'Could not load license entitlements.');
  }
  return parseEntitlements(data);
}

export async function ensurePersonalTeam(supabase) {
  const { data, error } = await supabase.rpc('ensure_personal_team');
  if (error) {
    throw new Error(error.message || 'Could not create your personal team.');
  }
  return data?.team_id ? String(data.team_id) : null;
}

export async function claimLicenseCode(supabase, code) {
  const normalized = String(code || '').trim();
  if (!normalized) return null;

  const { data, error } = await supabase.rpc('claim_license', { p_code: normalized });
  if (error) {
    throw new Error(error.message || 'Could not claim license code.');
  }
  return data;
}

export async function signInForCloudFeatures(email, password) {
  const supabase = getSupabaseClient();
  const normalizedEmail = email.trim();

  const { data: signInData, error: signInError } = await supabase.auth.signInWithPassword({
    email: normalizedEmail,
    password,
  });

  if (signInError) {
    throw new Error(getFriendlyAuthError(signInError));
  }

  if (!signInData.session) {
    throw new Error('Login failed. Please try again.');
  }

  const { data: userData, error: userError } = await supabase.auth.getUser();
  if (userError || !userData?.user) {
    return rejectSignedInUser(supabase, 'We could not identify this account. Please try again.');
  }

  const user = userData.user;
  let entitlements = await fetchUserEntitlements(supabase);
  const hasActiveLicense = Boolean(entitlements.plan && entitlements.status === 'active');

  const { data: profile, error: profileError } = await supabase
    .from('profiles')
    .select('id, role, display_name, avatar_url')
    .eq('id', user.id)
    .maybeSingle();

  if (profileError) {
    return rejectSignedInUser(supabase, 'We could not load your profile.');
  }

  let activeTeamId = entitlements.activeTeamId;
  let memberships = [];

  if (hasActiveLicense) {
    if (entitlements.plan === 'individual' && !activeTeamId) {
      activeTeamId = await ensurePersonalTeam(supabase);
      entitlements = await fetchUserEntitlements(supabase);
      activeTeamId = activeTeamId || entitlements.activeTeamId;
    }

    if (entitlements.plan === 'team' && entitlements.teamIds.length === 0) {
      return rejectSignedInUser(
        supabase,
        'This Team license account is not assigned to a team yet. Ask your PM for an invitation code.',
      );
    }

    const { data: membershipRows, error: membershipError } = await supabase
      .from('team_members')
      .select('team_id, role')
      .eq('user_id', user.id);

    if (membershipError) {
      return rejectSignedInUser(supabase, 'We could not load your team membership.');
    }

    memberships = membershipRows ?? [];
  }

  const membership =
    memberships.find((m) => m.team_id === activeTeamId) ??
    memberships[0] ??
    (activeTeamId ? { team_id: activeTeamId, role: 'owner' } : null);

  return {
    session: signInData.session,
    user,
    profile: profile ?? {
      id: user.id,
      role: 'user',
      display_name: user.email,
      avatar_url: null,
    },
    membership,
    memberships,
    entitlements: {
      plan: entitlements.plan,
      status: entitlements.status,
      team_ids: entitlements.teamIds,
      active_team_id: activeTeamId,
      can_sync: entitlements.canSync,
      can_cloud_ai: entitlements.canCloudAi,
      can_integrations: entitlements.canIntegrations,
    },
  };
}

/** @deprecated Use signInForCloudFeatures */
export const signInWorker = signInForCloudFeatures;

export async function signOutWorker() {
  const supabase = getSupabaseClient();
  await supabase.auth.signOut();
}

export { parseEntitlements, fetchUserEntitlements as getUserEntitlements };
