import React from "react";

/**
 * Local Voice AI word mark, built on the WAI design system.
 *
 * Follows the same lockup grammar as the Wolff Applied AI logo: mark on the
 * left, word mark beside it, endorsement as a small line underneath — never
 * stacked. The accent half of the name carries Signalgelb (#FFDD00), and on a
 * light background it gets the same 0.5px ink outline the WAI mark uses,
 * because yellow type on light alone is too low in contrast.
 *
 * This replaces the upstream Handy mark on purpose: Handy's code is MIT, its
 * name and logo are not (see docs/DECISIONS.md D2).
 */
const LocalVoiceAiLogo = ({
  width,
  height,
  className,
  showEndorsement = true,
  showMark = true,
}: {
  width?: number;
  height?: number;
  className?: string;
  showEndorsement?: boolean;
  /** The pictorial mark. Off in the sidebar, where the taskbar icon already
   *  carries it and the space belongs to the navigation labels. */
  showMark?: boolean;
}) => {
  return (
    <span
      className={`lva-logo ${className ?? ""}`}
      style={width ? { width } : undefined}
    >
      {showMark && <LocalVoiceAiMark height={height ?? 26} />}
      <span className="lva-logo__text">
        {/* Markenname und Endorsement sind Eigennamen — bewusst nicht übersetzt. */}
        {/* eslint-disable i18next/no-literal-string */}
        <span className="lva-logo__name">
          Local&nbsp;
          <span className="lva-logo__accent">Voice&nbsp;AI</span>
        </span>
        {showEndorsement && (
          <small className="lva-logo__endorsement">Ingenieurbüro Wolff</small>
        )}
        {/* eslint-enable i18next/no-literal-string */}
      </span>
    </span>
  );
};

/**
 * The pictorial mark: a spoken waveform crowned by a small AI spark — voice
 * in, intelligence on top. Sits on its own carrier tile, mirroring the WAI
 * solo-"W" rule that a yellow mark always needs a dark ground to be visible.
 */
export const LocalVoiceAiMark = ({
  height = 26,
  size: sizeProp,
  className,
}: {
  // Doubles as a sidebar icon, where the shared IconProps type allows a
  // string dimension — hence the wider type here.
  height?: number | string;
  size?: number | string;
  className?: string;
}) => {
  const size = sizeProp ?? height;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={`lva-mark ${className ?? ""}`}
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect className="lva-mark__tile" width="32" height="32" rx="7" />
      {/* Five waveform bars — the voice. */}
      <path
        className="lva-mark__wave"
        d="M7 14.5v5"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
      <path
        className="lva-mark__wave"
        d="M11.5 11.5v11"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
      <path
        className="lva-mark__wave"
        d="M16 9v16"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
      <path
        className="lva-mark__wave"
        d="M20.5 11.5v11"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
      <path
        className="lva-mark__wave"
        d="M25 14.5v5"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
      {/* The AI spark above the loudest bar. */}
      <circle className="lva-mark__dot" cx="25" cy="8.2" r="1.7" />
    </svg>
  );
};

export default LocalVoiceAiLogo;
