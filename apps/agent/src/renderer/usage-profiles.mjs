// Native entitlements are the authority. Remembering an onboarding choice never grants a role.
export const PROFILE_IDS = Object.freeze(['individual', 'team_employee']);
const personalCapabilities = Object.freeze({ online: true, focus_alerts: true, personal_insights: true, team_dashboard: false });
const safeCapabilities = Object.freeze({ online: false, focus_alerts: false, personal_insights: false, team_dashboard: false });

export function resolveUsageProfile(entitlements, { accountId = null, signedIn = Boolean(accountId), hydrated = true } = {}) {
  const local = {
    id: 'individual', verified: false, guest: !signedIn, local: true, team_id: null, team_name: null, can_manage_team: false,
    capabilities: { ...personalCapabilities }, available_profiles: ['individual'], teams: [],
  };
  if (hydrated && !signedIn) return local;
  const safe = {
    id: 'individual', verified: false, guest: false, local: false, team_id: null, team_name: null, can_manage_team: false,
    capabilities: { ...safeCapabilities }, available_profiles: [], teams: [],
  };
  const supplied = entitlements?.usage_profile;
  if (!hydrated || !accountId) return safe;
  // Account-scoped native caches survive network failures. A foreign cache has
  // no authority over this account's local mode or its team destination.
  if (entitlements?.owner_user_id && entitlements.owner_user_id !== accountId) return local;
  if (!supplied) {
    if (entitlements?.owner_user_id === accountId && (entitlements.plan === 'team' || entitlements.active_team_id && entitlements.plan !== 'individual')) return { ...safe, id: 'team_employee' };
    return local;
  }
  if (entitlements?.owner_user_id !== accountId || supplied.verified !== true) return { ...safe, id: supplied.id === 'team_employee' ? 'team_employee' : 'individual' };
  const teams = (Array.isArray(supplied.teams) ? supplied.teams : []).filter(team => typeof team?.id === 'string' && team.id && typeof team.name === 'string').map(team => ({ id: team.id, name: team.name, manager: team.manager === true }));
  const team = teams.find(candidate => candidate.id === supplied.team_id);
  // Older servers returned a manager recorder. Keep its personal features
  // locked until refresh, and send management to the existing web dashboard.
  if (supplied.id === 'team_manager') return { ...safe, teams, can_manage_team: Boolean(team?.manager) };
  if (!PROFILE_IDS.includes(supplied.id)) return safe;
  // A role belongs to a named, verified workspace; never construct it from a plan label.
  if (supplied.id === 'team_employee' && (!team || team.manager)) return { ...safe, id: 'team_employee' };
  const available = (Array.isArray(supplied.available_profiles) ? supplied.available_profiles : []).filter(id => PROFILE_IDS.includes(id) && (id === 'individual' || teams.some(candidate => !candidate.manager)));
  const capabilities = supplied.id === 'individual'
    ? Object.fromEntries(Object.keys(personalCapabilities).map(key => [key, supplied.capabilities?.[key] === true]))
    : { ...safeCapabilities };
  capabilities.team_dashboard = false;
  return { ...safe, id: supplied.id, verified: true, license_plan_id: supplied.license_plan_id ?? null,
    team_id: supplied.id === 'individual' ? null : team.id, team_name: supplied.id === 'individual' ? null : team.name,
    capabilities, available_profiles: available, teams, can_manage_team: teams.some(candidate => candidate.manager) };
}

export const isTeamProfile = profile => profile?.id === 'team_employee';
export function profileAllowsTab(profile, tabId) {
  if (tabId === 'tabToday' || tabId === 'tabProfile') return true;
  if (tabId === 'tabOnline') return !isTeamProfile(profile) && profile?.capabilities?.online === true;
  if (tabId === 'tabSummary' || tabId === 'tabCloudInsights') return !isTeamProfile(profile) && profile?.capabilities?.personal_insights === true;
  return false;
}

export function profileChoice(profile, id, selectedTeamId = null) {
  if (!PROFILE_IDS.includes(id) || !profile.available_profiles.includes(id)) return null;
  if (id === 'individual') return profile.local ? { mode: 'local', teamId: null } : { mode: 'individual', teamId: null };
  const candidates = profile.teams.filter(team => !team.manager);
  const team = candidates.find(candidate => candidate.id === selectedTeamId) ?? candidates.find(candidate => candidate.id === profile.team_id) ?? candidates[0];
  return team ? { mode: 'team', teamId: team.id } : null;
}

// Dialogs keep focus and background interaction separate, including a privacy notice
// opened above first-run setup. A timer never installs a global Space shortcut.
export function createDialogFocusController(document, { onEscape = {} } = {}) {
  const stack = [];
  const previousInert = new Map();
  const visible = element => element?.classList.contains('visible') && !element.hidden;
  function restoreBackground() {
    for (const [element, value] of previousInert) if (element.isConnected) element.inert = value;
    previousInert.clear();
  }
  function updateBackground() {
    restoreBackground();
    const active = stack.at(-1)?.element;
    if (!active) return;
    for (let element = active; element?.parentElement; element = element.parentElement) {
      for (const sibling of element.parentElement.children) {
        if (sibling === element || sibling.tagName === 'SCRIPT' || sibling.tagName === 'STYLE') continue;
        previousInert.set(sibling, sibling.inert);
        sibling.inert = true;
      }
      if (element.parentElement === document.body) break;
    }
  }
  function focusable(element) {
    return [...element.querySelectorAll('button, input, select, textarea, summary, a[href], [tabindex]')].filter(node => !node.disabled && !node.hidden && node.tabIndex >= 0 && node.getClientRects().length && getComputedStyle(node).visibility !== 'hidden' && !node.closest('[inert]'));
  }
  function open(element) {
    if (!element) return;
    const existing = stack.findIndex(item => item.element === element);
    if (existing !== -1) stack.splice(existing, 1);
    stack.push({ element, previousFocus: document.activeElement });
    element.classList.add('visible'); element.setAttribute('aria-hidden', 'false');
    updateBackground();
    const heading = element.querySelector('h1, h2');
    if (heading) heading.tabIndex = -1;
    (heading || focusable(element)[0] || element).focus({ preventScroll: true });
  }
  function close(element) {
    const index = stack.findIndex(item => item.element === element);
    const record = index !== -1 ? stack.splice(index, 1)[0] : null;
    element?.classList.remove('visible'); element?.setAttribute('aria-hidden', 'true');
    updateBackground();
    if (record?.previousFocus?.isConnected && !record.previousFocus.closest('[inert]') && record.previousFocus.getClientRects().length) record.previousFocus.focus({ preventScroll: true });
    else if (stack.length) (focusable(stack.at(-1).element)[0] || stack.at(-1).element).focus({ preventScroll: true });
  }
  document.addEventListener('keydown', event => {
    while (stack.length && !visible(stack.at(-1).element)) { stack.pop(); updateBackground(); }
    const active = stack.at(-1)?.element;
    if (!active) return;
    if (event.key === 'Escape' && onEscape[active.id]) {
      event.preventDefault(); event.stopPropagation(); onEscape[active.id](); return;
    }
    if (event.key !== 'Tab') return;
    const nodes = focusable(active), first = nodes[0], last = nodes.at(-1);
    if (!nodes.length) { event.preventDefault(); return; }
    if (!active.contains(document.activeElement) || !nodes.includes(document.activeElement)) {
      event.preventDefault(); (event.shiftKey ? last : first).focus();
    } else if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }, true);
  return { open, close };
}
