export interface Language {
  value: string;
  label: string;
}

export const CHINESE_LANGUAGE_CODE = "zh";

export const LANGUAGES: Language[] = [
  { value: "auto", label: "Auto Detect" },
  { value: "en", label: "English" },
  { value: CHINESE_LANGUAGE_CODE, label: "Chinese" },
  { value: "zh-Hans", label: "Chinese (Simplified)" },
  { value: "zh-Hant", label: "Chinese (Traditional)" },
  { value: "yue", label: "Cantonese" },
  { value: "de", label: "German" },
  { value: "es", label: "Spanish" },
  { value: "ru", label: "Russian" },
  { value: "ko", label: "Korean" },
  { value: "fr", label: "French" },
  { value: "ja", label: "Japanese" },
  { value: "pt", label: "Portuguese" },
  { value: "tr", label: "Turkish" },
  { value: "pl", label: "Polish" },
  { value: "ca", label: "Catalan" },
  { value: "nl", label: "Dutch" },
  { value: "ar", label: "Arabic" },
  { value: "sv", label: "Swedish" },
  { value: "it", label: "Italian" },
  { value: "id", label: "Indonesian" },
  { value: "hi", label: "Hindi" },
  { value: "fi", label: "Finnish" },
  { value: "vi", label: "Vietnamese" },
  { value: "he", label: "Hebrew" },
  { value: "uk", label: "Ukrainian" },
  { value: "el", label: "Greek" },
  { value: "ms", label: "Malay" },
  { value: "cs", label: "Czech" },
  { value: "ro", label: "Romanian" },
  { value: "da", label: "Danish" },
  { value: "hu", label: "Hungarian" },
  { value: "ta", label: "Tamil" },
  { value: "no", label: "Norwegian" },
  { value: "th", label: "Thai" },
  { value: "ur", label: "Urdu" },
  { value: "hr", label: "Croatian" },
  { value: "bg", label: "Bulgarian" },
  { value: "lt", label: "Lithuanian" },
  { value: "la", label: "Latin" },
  { value: "mi", label: "Maori" },
  { value: "ml", label: "Malayalam" },
  { value: "cy", label: "Welsh" },
  { value: "sk", label: "Slovak" },
  { value: "te", label: "Telugu" },
  { value: "fa", label: "Persian" },
  { value: "lv", label: "Latvian" },
  { value: "bn", label: "Bengali" },
  { value: "sr", label: "Serbian" },
  { value: "az", label: "Azerbaijani" },
  { value: "sl", label: "Slovenian" },
  { value: "kn", label: "Kannada" },
  { value: "et", label: "Estonian" },
  { value: "mk", label: "Macedonian" },
  { value: "br", label: "Breton" },
  { value: "eu", label: "Basque" },
  { value: "is", label: "Icelandic" },
  { value: "hy", label: "Armenian" },
  { value: "ne", label: "Nepali" },
  { value: "mn", label: "Mongolian" },
  { value: "bs", label: "Bosnian" },
  { value: "kk", label: "Kazakh" },
  { value: "sq", label: "Albanian" },
  { value: "sw", label: "Swahili" },
  { value: "gl", label: "Galician" },
  { value: "mr", label: "Marathi" },
  { value: "pa", label: "Punjabi" },
  { value: "si", label: "Sinhala" },
  { value: "km", label: "Khmer" },
  { value: "sn", label: "Shona" },
  { value: "yo", label: "Yoruba" },
  { value: "so", label: "Somali" },
  { value: "af", label: "Afrikaans" },
  { value: "oc", label: "Occitan" },
  { value: "ka", label: "Georgian" },
  { value: "be", label: "Belarusian" },
  { value: "tg", label: "Tajik" },
  { value: "sd", label: "Sindhi" },
  { value: "gu", label: "Gujarati" },
  { value: "am", label: "Amharic" },
  { value: "yi", label: "Yiddish" },
  { value: "lo", label: "Lao" },
  { value: "uz", label: "Uzbek" },
  { value: "fo", label: "Faroese" },
  { value: "ht", label: "Haitian Creole" },
  { value: "ps", label: "Pashto" },
  { value: "tk", label: "Turkmen" },
  { value: "nn", label: "Nynorsk" },
  { value: "mt", label: "Maltese" },
  { value: "sa", label: "Sanskrit" },
  { value: "lb", label: "Luxembourgish" },
  { value: "my", label: "Myanmar" },
  { value: "bo", label: "Tibetan" },
  { value: "tl", label: "Tagalog" },
  { value: "mg", label: "Malagasy" },
  { value: "as", label: "Assamese" },
  { value: "tt", label: "Tatar" },
  { value: "haw", label: "Hawaiian" },
  { value: "ln", label: "Lingala" },
  { value: "ha", label: "Hausa" },
  { value: "ba", label: "Bashkir" },
  { value: "jw", label: "Javanese" },
  { value: "su", label: "Sundanese" },
];

const CHINESE_OUTPUT_INTENTS = new Set(["zh-Hans", "zh-Hant"]);

const LANGUAGE_LABELS = new Map(
  LANGUAGES.map((language) => [language.value, language.label] as const),
);

export const MODEL_CAPABILITY_LANGUAGES: Language[] = LANGUAGES.filter(
  (language) =>
    language.value !== "auto" && !CHINESE_OUTPUT_INTENTS.has(language.value),
);

// Languages offered in the transcription-language picker. We surface the two
// explicit Chinese *output* variants (Simplified / Traditional) and hide the
// bare recognition code `zh` ("Chinese"): all three recognize identically, so
// the plain option only adds ambiguity about which script you get. `zh` stays in
// LANGUAGES — it's still a valid *effective* language (auto-detect and must-pick
// fallback can resolve to it) and its label is needed to render that state — it
// just isn't directly selectable.
export const SELECTABLE_LANGUAGES: Language[] = LANGUAGES.filter(
  (language) => language.value !== CHINESE_LANGUAGE_CODE,
);

// Collapse a language tag to the base code Handy matches on, dropping any
// BCP-47 region or script subtag: "en-US" → "en", "zh-CN" → "zh", "zh-Hant" →
// "zh". Bare and three-letter codes ("haw") pass through unchanged. This lets
// the picker match a model's *real* codes — which may be full locales like
// "en-US" (e.g. Nemotron Streaming) — against Handy's canonical bare-code
// LANGUAGES list without the backend having to mangle the codes the engine needs.
export const recognitionLanguage = (languageCode: string): string => {
  const separatorIndex = languageCode.indexOf("-");
  return separatorIndex === -1
    ? languageCode
    : languageCode.slice(0, separatorIndex);
};

export const supportsLanguageCode = (
  supportedLanguages: string[],
  languageCode: string,
): boolean => {
  const recognitionCode = recognitionLanguage(languageCode);
  return supportedLanguages.some(
    (supportedLanguage) =>
      recognitionLanguage(supportedLanguage) === recognitionCode,
  );
};

export const getUniqueCapabilityLanguages = (
  supportedLanguages: string[],
): string[] => {
  const seen = new Set<string>();
  return supportedLanguages.map(recognitionLanguage).filter((languageCode) => {
    if (seen.has(languageCode)) return false;
    seen.add(languageCode);
    return true;
  });
};

export const getLanguageLabel = (languageCode: string): string | undefined =>
  LANGUAGE_LABELS.get(languageCode);

/// Zielsprachen der Übersetzung, als englische Namen — so erwartet sie der
/// Übersetzungs-Prompt. Die Beschriftungen sind Eigennamen der Sprachen und
/// deshalb keine UI-Texte, die übersetzt würden.
///
/// An dieser Stelle, weil zwei Bildschirme sie brauchen: das Vorlesen (Reiter
/// Übersetzung) und die Audio-Übersetzung.
/// `code` ist der BCP-47-Sprachcode. Er landet als `lang`-Attribut am
/// Übersetzungsfeld, damit die Rechtschreibprüfung des WebViews in der
/// Zielsprache prüft — ohne ihn prüft sie in der App-Sprache und
/// unterstreicht eine englische Übersetzung komplett rot.
export const TTS_TARGET_LANGS = [
  { value: "German", label: "Deutsch", code: "de" },
  { value: "English", label: "English", code: "en" },
  { value: "French", label: "Français", code: "fr" },
  { value: "Spanish", label: "Español", code: "es" },
  { value: "Italian", label: "Italiano", code: "it" },
  { value: "Portuguese", label: "Português", code: "pt" },
  { value: "Dutch", label: "Nederlands", code: "nl" },
  { value: "Polish", label: "Polski", code: "pl" },
  { value: "Japanese", label: "日本語", code: "ja" },
  { value: "Chinese", label: "中文", code: "zh" },
];

/// Sprachcode zur Zielsprache — Rückfall Englisch, nie die App-Sprache:
/// eine unbekannte Zielsprache ist sicher nicht Deutsch.
export const targetLangCode = (value: string): string =>
  TTS_TARGET_LANGS.find((l) => l.value === value)?.code ?? "en";
