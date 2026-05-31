pub struct TokenMatrix {
    pub openai_gpt4o: usize,
    pub anthropic_claude: usize,
    pub google_gemini: usize,
}

pub struct TokenCounter;

impl TokenCounter {
    pub fn calculate_all(text: &str) -> TokenMatrix {
        let bpe = tiktoken_rs::cl100k_base().unwrap(); // หรือ o200k_base
        let openai_tokens = bpe.encode_with_special_tokens(text).len();

        let anthropic_tokens = (openai_tokens as f64 * 0.96) as usize; 
        let gemini_tokens = (openai_tokens as f64 * 1.02) as usize;

        TokenMatrix {
            openai_gpt4o: openai_tokens,
            anthropic_claude: anthropic_tokens,
            google_gemini: gemini_tokens,
        }
    }
}