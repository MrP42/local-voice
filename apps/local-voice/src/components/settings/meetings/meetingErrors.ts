import type { TFunction } from "i18next";

/**
 * Backend error/status codes for meeting recording and import, mapped to
 * their i18n keys. The recorder's `delta_store_failed` command error comes
 * back as `"delta_store_failed: <details>"` (the store's own error message
 * is appended after a colon), while the same code arrives verbatim as an
 * event payload — so the code is normalized by taking everything before the
 * first colon before looking it up.
 */
const ERROR_KEY_MAP: Record<string, string> = {
  consent_required: "meetings.errors.consentRequired",
  dictation_active: "meetings.errors.dictationActive",
  delta_store_failed: "meetings.errors.deltaStoreFailed",
  chunk_transcription_failed: "meetings.errors.chunkTranscriptionFailed",
  loopback_start_timeout: "meetings.errors.loopbackStartTimeout",
};

/**
 * Translates a raw backend error/status code into a user-facing message.
 * Unknown codes (arbitrary I/O or DB error strings that aren't part of the
 * fixed vocabulary above) fall back to the raw string rather than hiding
 * information the user might need to report a bug.
 */
export const translateMeetingError = (code: string, t: TFunction): string => {
  const normalized = code.split(":")[0].trim();
  const key = ERROR_KEY_MAP[normalized];
  return key ? t(key) : code;
};
