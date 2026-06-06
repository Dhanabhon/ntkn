use std::collections::HashMap;

pub struct ModelDetails {
    pub display_name: String,
    pub limit: usize,
}

pub fn resolve_model_details(
    model_name: &str,
    provider: &str,
    custom_limits: &Option<HashMap<String, usize>>,
) -> ModelDetails {
    let name_lower = model_name.to_lowercase();

    // Check if custom limits override exists for this specific model name
    if let Some(limits) = custom_limits {
        if let Some(&limit) = limits.get(model_name) {
            return ModelDetails {
                display_name: model_name.to_string(),
                limit,
            };
        }
        // Also check lowercased
        if let Some(&limit) = limits.get(&name_lower) {
            return ModelDetails {
                display_name: model_name.to_string(),
                limit,
            };
        }
    }

    let is_provider_match = match provider {
        "openai" => {
            name_lower.contains("gpt")
                || name_lower.contains("openai")
                || name_lower.contains("o1")
                || name_lower.contains("o3")
        }
        "anthropic" => name_lower.contains("claude") || name_lower.contains("anthropic"),
        "google" => name_lower.contains("gemini") || name_lower.contains("google"),
        _ => false,
    };

    if !is_provider_match {
        // If the active model belongs to a different provider, this provider should display its flagship default
        return match provider {
            "openai" => ModelDetails {
                display_name: "GPT-4o".to_string(),
                limit: 128_000,
            },
            "anthropic" => ModelDetails {
                display_name: "Claude 3.5 Sonnet".to_string(),
                limit: 200_000,
            },
            "google" => ModelDetails {
                display_name: "Gemini 1.5/2.0".to_string(),
                limit: 1_000_000,
            },
            _ => ModelDetails {
                display_name: "Unknown".to_string(),
                limit: 100_000,
            },
        };
    }

    // Clean up aider prefixes if present (e.g. "openai/gpt-4o" -> "gpt-4o", "anthropic/claude-3-5-sonnet" -> "claude-3-5-sonnet")
    let clean_name = if let Some(slash_idx) = model_name.find('/') {
        &model_name[slash_idx + 1..]
    } else {
        model_name
    };

    let clean_name_lower = clean_name.to_lowercase();

    match provider {
        "openai" => {
            let limit = if clean_name_lower.contains("gpt-4o-mini") {
                128_000
            } else if clean_name_lower.contains("gpt-4o") || clean_name_lower.contains("gpt-4-turbo") {
                128_000
            } else if clean_name_lower.contains("o1") || clean_name_lower.contains("o3") {
                if clean_name_lower.contains("preview") || clean_name_lower.contains("mini") {
                    128_000
                } else {
                    200_000
                }
            } else if clean_name_lower.contains("gpt-4-32k") {
                32_768
            } else if clean_name_lower.contains("gpt-4") {
                8_192
            } else if clean_name_lower.contains("gpt-3.5-turbo") {
                16_385
            } else {
                128_000 // default
            };

            ModelDetails {
                display_name: clean_name.to_string(),
                limit,
            }
        }
        "anthropic" => {
            let limit = if clean_name_lower.contains("claude-3-5") || clean_name_lower.contains("claude-3.5") {
                200_000
            } else if clean_name_lower.contains("claude-3") {
                200_000
            } else if clean_name_lower.contains("claude-2.1") {
                200_000
            } else if clean_name_lower.contains("claude-2.0") || clean_name_lower.contains("claude-2") {
                100_000
            } else if clean_name_lower.contains("claude-instant") {
                100_000
            } else {
                200_000
            };

            ModelDetails {
                display_name: clean_name.to_string(),
                limit,
            }
        }
        "google" => {
            let limit = if clean_name_lower.contains("gemini-1.5-pro") || clean_name_lower.contains("gemini-2.5-pro") {
                2_097_152
            } else if clean_name_lower.contains("gemini-1.5-flash") || clean_name_lower.contains("gemini-2.0-flash") || clean_name_lower.contains("gemini-2.5-flash") {
                1_048_576
            } else if clean_name_lower.contains("gemini-1.0-pro") {
                32_768
            } else if clean_name_lower.contains("pro") {
                2_097_152
            } else {
                1_048_576
            };

            ModelDetails {
                display_name: clean_name.to_string(),
                limit,
            }
        }
        _ => ModelDetails {
            display_name: clean_name.to_string(),
            limit: 100_000,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_openai() {
        let details = resolve_model_details("gpt-3.5-turbo", "openai", &None);
        assert_eq!(details.display_name, "gpt-3.5-turbo");
        assert_eq!(details.limit, 16_385);

        let details_aider = resolve_model_details("openai/gpt-4o", "openai", &None);
        assert_eq!(details_aider.display_name, "gpt-4o");
        assert_eq!(details_aider.limit, 128_000);
    }

    #[test]
    fn test_resolve_anthropic() {
        let details = resolve_model_details("claude-3-5-sonnet-20241022", "anthropic", &None);
        assert_eq!(details.display_name, "claude-3-5-sonnet-20241022");
        assert_eq!(details.limit, 200_000);
    }

    #[test]
    fn test_resolve_gemini() {
        let details = resolve_model_details("gemini-1.5-flash", "google", &None);
        assert_eq!(details.display_name, "gemini-1.5-flash");
        assert_eq!(details.limit, 1_048_576);

        let details_pro = resolve_model_details("gemini-1.5-pro", "google", &None);
        assert_eq!(details_pro.limit, 2_097_152);
    }

    #[test]
    fn test_custom_limits() {
        let mut custom = HashMap::new();
        custom.insert("custom-model".to_string(), 999_999);
        let details = resolve_model_details("custom-model", "openai", &Some(custom));
        assert_eq!(details.display_name, "custom-model");
        assert_eq!(details.limit, 999_999);
    }
}
