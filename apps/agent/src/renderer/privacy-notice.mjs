// The September local build explains retention under the same tracking notice.
// Keep this list aligned with privacy.rs; unknown notices still need acceptance.
const compatibleVersions = new Set(['2026-08-23', '2026-09-28']);

export function monitoringNoticeAccepted(settings) {
  return settings?.monitoringNoticeAcknowledged === true
    && compatibleVersions.has(settings.noticeVersion);
}
