// Supabase Cloud plans; the one-time FSI- local license is a separate product.
const PAID_CALENDAR_PLANS = new Set(['pro', 'individual', 'individual_pro']);

export function hasPaidCalendarEntitlement(entitlements, userId) {
  return Boolean(
    userId &&
    entitlements?.owner_user_id === userId &&
    entitlements?.status === 'active' &&
    entitlements?.can_integrations === true &&
    PAID_CALENDAR_PLANS.has(entitlements?.plan)
  );
}
