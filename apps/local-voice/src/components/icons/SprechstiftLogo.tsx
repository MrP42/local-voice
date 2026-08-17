import React from "react";

/**
 * Sprechstift word mark, built on the WAI design system.
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
const SprechstiftLogo = ({
  width,
  height,
  className,
  showEndorsement = true,
}: {
  width?: number;
  height?: number;
  className?: string;
  showEndorsement?: boolean;
}) => {
  return (
    <span
      className={`sprechstift-logo ${className ?? ""}`}
      style={width ? { width } : undefined}
    >
      <SprechstiftMark height={height ?? 26} />
      <span className="sprechstift-logo__text">
        <span className="sprechstift-logo__name">
          Sprech<span className="sprechstift-logo__accent">stift</span>
        </span>
        {showEndorsement && (
          <small className="sprechstift-logo__endorsement">
            Ingenieurbüro Wolff
          </small>
        )}
      </span>
    </span>
  );
};

/**
 * The pictorial mark: a pen nib whose tip emits speech waves — dictation as
 * writing. Sits on its own carrier tile, mirroring the WAI solo-"W" rule that
 * a yellow mark always needs a dark ground to be visible.
 */
export const SprechstiftMark = ({
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
      className={`sprechstift-mark ${className ?? ""}`}
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect className="sprechstift-mark__tile" width="32" height="32" rx="7" />
      {/* Pen body, angled like a held pen. */}
      <path
        className="sprechstift-mark__nib"
        d="M19.6 7.4 22.9 10.7 13.2 20.4 8.6 21.7 9.9 17.1z"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      {/* Two speech arcs off the writing tip. */}
      <path
        className="sprechstift-mark__wave"
        d="M23.3 15.1a4.6 4.6 0 0 1 0 6.2"
        strokeWidth="1.7"
        strokeLinecap="round"
        fill="none"
      />
      <path
        className="sprechstift-mark__wave"
        d="M26.1 12.6a8.3 8.3 0 0 1 0 11.2"
        strokeWidth="1.7"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  );
};

export default SprechstiftLogo;
