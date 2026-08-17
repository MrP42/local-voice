use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextKey {
    pub(crate) foreground: isize,
    pub(crate) focus: isize,
    pub(crate) physical_generation: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedSnapshot {
    pub(crate) run_id: u64,
    pub(crate) text: String,
    pub(crate) context: ContextKey,
}

#[derive(Debug)]
pub(crate) struct ReplacementPlan {
    expected_rendered: String,
    resulting_rendered: String,
    edit_start: usize,
    edit_original_end: usize,
    edit_candidate_end: usize,
    clear_sentences: bool,
    pub(crate) select_chars: usize,
    pub(crate) replacement: String,
}

#[derive(Debug, Clone)]
struct RegisteredSentence {
    original: String,
    start: usize,
    end: usize,
}

#[derive(Debug)]
pub(crate) struct RunState {
    run_id: u64,
    rendered: String,
    context: Option<ContextKey>,
    safe: bool,
    sealed: bool,
    cancelled: bool,
    sentences: HashMap<u64, RegisteredSentence>,
}

impl RunState {
    pub(crate) fn new(run_id: u64, refinement_enabled: bool) -> Self {
        Self {
            run_id,
            rendered: String::new(),
            context: None,
            safe: refinement_enabled,
            sealed: false,
            cancelled: false,
            sentences: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn rendered_text(&self) -> &str {
        &self.rendered
    }

    pub(crate) fn is_run(&self, run_id: u64) -> bool {
        self.run_id == run_id
    }

    pub(crate) fn wants_context(&self) -> bool {
        self.safe && !self.cancelled
    }

    pub(crate) fn record_append(
        &mut self,
        fragment: &str,
        before: Option<ContextKey>,
        after: Option<ContextKey>,
        paste_succeeded: bool,
    ) {
        let was_empty = self.rendered.is_empty();
        self.rendered.push_str(fragment);

        if !self.safe || !paste_succeeded {
            self.safe = false;
            return;
        }
        let (Some(before), Some(after)) = (before, after) else {
            self.safe = false;
            return;
        };
        if before != after {
            self.safe = false;
            return;
        }

        if was_empty {
            self.context = Some(after);
        } else if self.context != Some(before) {
            self.safe = false;
        }
    }

    pub(crate) fn register_sentence(&mut self, run_id: u64, sentence_id: u64, original: &str) {
        if run_id != self.run_id
            || !self.safe
            || self.sealed
            || self.cancelled
            || original.is_empty()
            || self.sentences.contains_key(&sentence_id)
        {
            return;
        }

        let mut search_from = 0;
        while let Some(relative) = self.rendered[search_from..].find(original) {
            let start = search_from + relative;
            let end = start + original.len();
            let overlaps_registered = self
                .sentences
                .values()
                .any(|sentence| start < sentence.end && sentence.start < end);
            if !overlaps_registered {
                self.sentences.insert(
                    sentence_id,
                    RegisteredSentence {
                        original: original.to_string(),
                        start,
                        end,
                    },
                );
                return;
            }
            search_from = start + self.rendered[start..].chars().next().unwrap().len_utf8();
        }
    }

    pub(crate) fn plan_sentence(
        &self,
        run_id: u64,
        sentence_id: u64,
        original: &str,
        candidate: &str,
        current: ContextKey,
    ) -> Option<ReplacementPlan> {
        if run_id != self.run_id
            || !self.safe
            || self.sealed
            || self.cancelled
            || self.context != Some(current)
            || original.is_empty()
            || candidate.is_empty()
            || candidate == original
        {
            return None;
        }

        let sentence = self.sentences.get(&sentence_id)?;
        if sentence.original != original
            || self.rendered.get(sentence.start..sentence.end)? != original
        {
            return None;
        }

        // Refinement is asynchronous, so more committed text may have been
        // appended after this sentence while the model was running. A foreign
        // application only lets us select backwards from the caret. Replace
        // the entire app-owned suffix and paste the unchanged trailing text
        // back after the refined sentence; the queue keeps this atomic with
        // respect to later appends.
        let sentence_start = sentence.start;
        let sentence_end = sentence.end;
        let trailing = self.rendered.get(sentence_end..)?;
        let selected = self.rendered.get(sentence_start..)?;

        let mut resulting_rendered = self.rendered[..sentence_start].to_string();
        resulting_rendered.push_str(candidate);
        resulting_rendered.push_str(trailing);

        let mut replacement = candidate.to_string();
        replacement.push_str(trailing);
        Some(ReplacementPlan {
            expected_rendered: self.rendered.clone(),
            resulting_rendered,
            edit_start: sentence_start,
            edit_original_end: sentence_end,
            edit_candidate_end: sentence_start + candidate.len(),
            clear_sentences: false,
            select_chars: selected.chars().count(),
            replacement,
        })
    }

    pub(crate) fn prepare_final(
        &mut self,
        run_id: u64,
        current: Option<ContextKey>,
    ) -> Option<PreparedSnapshot> {
        if run_id != self.run_id || self.cancelled {
            return None;
        }
        self.sealed = true;
        let current = current?;
        if !self.safe || self.context != Some(current) || self.rendered.is_empty() {
            return None;
        }
        Some(PreparedSnapshot {
            run_id,
            text: self.rendered.clone(),
            context: current,
        })
    }

    pub(crate) fn plan_final(
        &self,
        run_id: u64,
        snapshot: &PreparedSnapshot,
        candidate: &str,
        current: ContextKey,
    ) -> Option<ReplacementPlan> {
        if run_id != self.run_id
            || snapshot.run_id != run_id
            || !self.safe
            || !self.sealed
            || self.cancelled
            || self.context != Some(current)
            || snapshot.context != current
            || snapshot.text != self.rendered
            || candidate.is_empty()
            || candidate == snapshot.text
        {
            return None;
        }
        Some(ReplacementPlan {
            expected_rendered: self.rendered.clone(),
            resulting_rendered: candidate.to_string(),
            edit_start: 0,
            edit_original_end: self.rendered.len(),
            edit_candidate_end: candidate.len(),
            clear_sentences: true,
            select_chars: self.rendered.chars().count(),
            replacement: candidate.to_string(),
        })
    }

    pub(crate) fn commit(&mut self, plan: ReplacementPlan) {
        if self.rendered == plan.expected_rendered {
            if plan.clear_sentences {
                self.sentences.clear();
            } else {
                let byte_delta = plan.edit_candidate_end as isize - plan.edit_original_end as isize;
                let mut updated_sentences = self.sentences.clone();
                for sentence in updated_sentences.values_mut() {
                    if sentence.end <= plan.edit_start {
                        continue;
                    }
                    if sentence.start >= plan.edit_original_end {
                        let (Some(start), Some(end)) = (
                            sentence.start.checked_add_signed(byte_delta),
                            sentence.end.checked_add_signed(byte_delta),
                        ) else {
                            self.safe = false;
                            return;
                        };
                        sentence.start = start;
                        sentence.end = end;
                    } else if sentence.start == plan.edit_start
                        && sentence.end == plan.edit_original_end
                    {
                        sentence.end = plan.edit_candidate_end;
                    } else {
                        self.safe = false;
                        return;
                    }
                }
                self.sentences = updated_sentences;
            }
            self.rendered = plan.resulting_rendered;
        } else {
            self.safe = false;
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.safe = false;
    }

    pub(crate) fn cancel(&mut self, run_id: u64) {
        if run_id == self.run_id {
            self.cancelled = true;
            self.sealed = true;
            self.safe = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextKey, RunState};

    fn context() -> ContextKey {
        ContextKey {
            foreground: 10,
            focus: 20,
            physical_generation: 30,
        }
    }

    #[test]
    fn sentence_replacement_is_planned_for_the_terminal_exact_text() {
        let mut state = RunState::new(7, true);
        state.record_append("Erster Satz.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Erster Satz.");

        let plan = state
            .plan_sentence(7, 1, "Erster Satz.", "Erster guter Satz.", context())
            .unwrap();
        assert_eq!(plan.select_chars, 12);
        assert_eq!(plan.replacement, "Erster guter Satz.");
        state.commit(plan);
        assert_eq!(state.rendered_text(), "Erster guter Satz.");
    }

    #[test]
    fn sentence_replacement_preserves_text_appended_while_model_was_running() {
        let mut state = RunState::new(7, true);
        state.record_append("Erster Satz.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Erster Satz.");
        state.record_append(" Danach.", Some(context()), Some(context()), true);

        let plan = state
            .plan_sentence(7, 1, "Erster Satz.", "Erster guter Satz.", context())
            .unwrap();
        assert_eq!(plan.select_chars, "Erster Satz. Danach.".chars().count());
        assert_eq!(plan.replacement, "Erster guter Satz. Danach.");
        state.commit(plan);
        assert_eq!(state.rendered_text(), "Erster guter Satz. Danach.");
    }

    #[test]
    fn registered_sentence_ranges_survive_an_earlier_suffix_rewrite() {
        let mut state = RunState::new(7, true);
        state.record_append("Test. Test.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Test.");
        state.register_sentence(7, 2, "Test.");

        let first = state
            .plan_sentence(7, 1, "Test.", "Erster Test.", context())
            .unwrap();
        state.commit(first);

        let second = state
            .plan_sentence(7, 2, "Test.", "Zweiter Test.", context())
            .unwrap();
        assert_eq!(second.select_chars, "Test.".chars().count());
        state.commit(second);
        assert_eq!(state.rendered_text(), "Erster Test. Zweiter Test.");
    }

    #[test]
    fn replacement_rejects_changed_window_focus_or_physical_input() {
        let mut state = RunState::new(7, true);
        state.record_append("Text.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Text.");

        for changed in [
            ContextKey {
                foreground: 11,
                ..context()
            },
            ContextKey {
                focus: 21,
                ..context()
            },
            ContextKey {
                physical_generation: 31,
                ..context()
            },
        ] {
            assert!(state
                .plan_sentence(7, 1, "Text.", "Text!", changed)
                .is_none());
        }
    }

    #[test]
    fn failed_or_unobservable_append_makes_run_fail_closed() {
        let mut failed = RunState::new(7, true);
        failed.record_append("Text.", Some(context()), Some(context()), false);
        failed.register_sentence(7, 1, "Text.");
        assert!(failed
            .plan_sentence(7, 1, "Text.", "Text!", context())
            .is_none());

        let mut unobservable = RunState::new(7, true);
        unobservable.record_append("Text.", None, None, true);
        unobservable.register_sentence(7, 1, "Text.");
        assert!(unobservable
            .plan_sentence(7, 1, "Text.", "Text!", context())
            .is_none());
    }

    #[test]
    fn final_snapshot_seals_sentences_and_requires_exact_state() {
        let mut state = RunState::new(7, true);
        state.record_append("Gesamter Text.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Gesamter Text.");

        let snapshot = state.prepare_final(7, Some(context())).unwrap();
        assert_eq!(snapshot.text, "Gesamter Text.");
        assert!(state
            .plan_sentence(7, 1, "Gesamter Text.", "Besser.", context())
            .is_none());

        let plan = state
            .plan_final(7, &snapshot, "Gesamter besserer Text.", context())
            .unwrap();
        assert_eq!(plan.select_chars, 14);
        state.commit(plan);
        assert_eq!(state.rendered_text(), "Gesamter besserer Text.");
    }

    #[test]
    fn wrong_run_or_cancel_rejects_late_results() {
        let mut state = RunState::new(7, true);
        state.record_append("Text.", Some(context()), Some(context()), true);
        state.register_sentence(7, 1, "Text.");
        assert!(state
            .plan_sentence(8, 1, "Text.", "Text!", context())
            .is_none());

        state.cancel(7);
        assert!(state
            .plan_sentence(7, 1, "Text.", "Text!", context())
            .is_none());
        assert!(state.prepare_final(7, Some(context())).is_none());
    }
}
