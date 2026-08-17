/**
 * Compares a dictated transcript against the phrase that was supposed to be
 * spoken, at word level.
 *
 * Normalisation is deliberately forgiving about the things a speech model may
 * legitimately render either way — punctuation, capitalisation, and the German
 * ß/ss and umlaut spellings — and strict about everything else. Number words
 * are NOT folded onto digits here: whether "dritten" or "3." comes back is a
 * real, visible difference between models (Parakeet normalises, Nemotron does
 * not), and hiding it would defeat the purpose of the bench.
 */

export type WordDiff = {
  kind: "same" | "missing" | "extra" | "different";
  text: string;
};

export type CompareResult = {
  accuracy: number;
  total: number;
  correct: number;
  substitutions: number;
  deletions: number;
  insertions: number;
  diff: WordDiff[];
};

export function normalizeWord(word: string): string {
  return word
    .toLowerCase()
    .replace(/[.,;:!?„“"'»«()\[\]]/g, "")
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss")
    .trim();
}

function words(text: string): string[] {
  return text.split(/\s+/).filter((w) => w.length > 0);
}

/**
 * Word-level alignment via the standard edit-distance backtrace, which is what
 * word error rate is defined on. A greedy diff would report a single inserted
 * word as "everything after it is wrong".
 */
export function compareTranscript(
  reference: string,
  heard: string,
): CompareResult {
  const ref = words(reference);
  const hyp = words(heard);
  const refNorm = ref.map(normalizeWord);
  const hypNorm = hyp.map(normalizeWord);

  const n = ref.length;
  const m = hyp.length;

  if (n === 0) {
    return {
      accuracy: m === 0 ? 1 : 0,
      total: 0,
      correct: 0,
      substitutions: 0,
      deletions: 0,
      insertions: m,
      diff: hyp.map((w) => ({ kind: "extra" as const, text: w })),
    };
  }

  // d[i][j] = edits turning the first i reference words into the first j heard.
  const d: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0),
  );
  for (let i = 0; i <= n; i++) d[i][0] = i;
  for (let j = 0; j <= m; j++) d[0][j] = j;
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      const cost = refNorm[i - 1] === hypNorm[j - 1] ? 0 : 1;
      d[i][j] = Math.min(
        d[i - 1][j] + 1, // deletion: a reference word was not heard
        d[i][j - 1] + 1, // insertion: a word was heard that was not said
        d[i - 1][j - 1] + cost,
      );
    }
  }

  const diff: WordDiff[] = [];
  let substitutions = 0;
  let deletions = 0;
  let insertions = 0;
  let correct = 0;

  let i = n;
  let j = m;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0) {
      const cost = refNorm[i - 1] === hypNorm[j - 1] ? 0 : 1;
      if (d[i][j] === d[i - 1][j - 1] + cost) {
        if (cost === 0) {
          correct++;
          diff.push({ kind: "same", text: hyp[j - 1] });
        } else {
          substitutions++;
          diff.push({ kind: "different", text: hyp[j - 1] });
        }
        i--;
        j--;
        continue;
      }
    }
    if (i > 0 && d[i][j] === d[i - 1][j] + 1) {
      deletions++;
      diff.push({ kind: "missing", text: ref[i - 1] });
      i--;
      continue;
    }
    insertions++;
    diff.push({ kind: "extra", text: hyp[j - 1] });
    j--;
  }
  diff.reverse();

  // Word error rate is errors over reference length; accuracy is its
  // complement, floored at 0 because insertions alone can push WER above 1.
  const errors = substitutions + deletions + insertions;
  const accuracy = Math.max(0, 1 - errors / n);

  return {
    accuracy,
    total: n,
    correct,
    substitutions,
    deletions,
    insertions,
    diff,
  };
}
