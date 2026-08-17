const EMBEDDING_NAME_PATTERNS: &[&str] = &["embed", "bge", "gte", "nomic-embed", "all-minilm"];
const MODEL_FAMILY_PREFERENCE: &[&str] =
    &["gemma4", "qwen3.5", "qwen3", "llama3.1", "mistral", "phi4"];

pub(crate) fn select_model(installed: &[String], configured: Option<&str>) -> Option<String> {
    if let Some(configured) = configured {
        return installed
            .iter()
            .find(|name| name.as_str() == configured && is_eligible(name))
            .cloned();
    }

    MODEL_FAMILY_PREFERENCE.iter().find_map(|family| {
        installed
            .iter()
            .find(|name| is_eligible(name) && belongs_to_family(name, family))
            .cloned()
    })
}

fn is_eligible(name: &str) -> bool {
    let lowercase = name.to_lowercase();
    let cloud_tag = lowercase
        .split_once(':')
        .is_some_and(|(_, tag)| tag.contains("cloud"));
    !cloud_tag
        && !EMBEDDING_NAME_PATTERNS
            .iter()
            .any(|pattern| lowercase.contains(pattern))
}

fn belongs_to_family(name: &str, family: &str) -> bool {
    let lowercase = name.to_lowercase();
    let without_tag = lowercase.split(':').next().unwrap_or_default();
    let base = without_tag.rsplit('/').next().unwrap_or_default();
    base == family
        || base
            .strip_prefix(family)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with('_'))
}

#[cfg(test)]
mod tests {
    use super::select_model;

    fn installed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn configured_model_is_exact_and_never_falls_back() {
        let models = installed(&["gemma4:12b", "mistral:7b"]);

        assert_eq!(
            select_model(&models, Some("mistral:7b")).as_deref(),
            Some("mistral:7b")
        );
        assert_eq!(select_model(&models, Some("Mistral:7b")), None);
        assert_eq!(select_model(&models, Some("missing:latest")), None);
    }

    #[test]
    fn configured_cloud_or_embedding_model_is_ineligible() {
        let models = installed(&["kimi-k3:cloud", "nomic-embed-text:latest"]);

        assert_eq!(select_model(&models, Some("kimi-k3:cloud")), None);
        assert_eq!(select_model(&models, Some("nomic-embed-text:latest")), None);
    }

    #[test]
    fn automatic_selection_filters_then_uses_family_preference() {
        let models = installed(&[
            "nomic-embed-text:latest",
            "kimi-k3:cloud",
            "qwen3-vl:235b-cloud",
            "qwen3:4b",
            "mistral:7b",
            "gemma4:12b",
        ]);

        assert_eq!(select_model(&models, None).as_deref(), Some("gemma4:12b"));
    }

    #[test]
    fn automatic_selection_rejects_cloud_suffixes_inside_a_tag() {
        let models = installed(&["qwen3-vl:235b-cloud", "mistral:7b"]);

        assert_eq!(select_model(&models, None).as_deref(), Some("mistral:7b"));
    }

    #[test]
    fn automatic_selection_preserves_tag_order_within_a_family() {
        let models = installed(&["qwen3:4b", "qwen3:8b", "mistral:7b"]);

        assert_eq!(select_model(&models, None).as_deref(), Some("qwen3:4b"));
    }

    #[test]
    fn automatic_selection_skips_when_no_preferred_chat_model_exists() {
        let models = installed(&[
            "nomic-embed-text:latest",
            "bge-m3:latest",
            "gte-large:latest",
            "all-minilm:latest",
            "kimi-k3:cloud",
            "unlisted-chat:7b",
        ]);

        assert_eq!(select_model(&models, None), None);
    }
}
