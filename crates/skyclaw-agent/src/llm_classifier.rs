//! LLM-based message classifier — classifies user messages as "chat", "order",
//! or "stop" using a single fast LLM call.
//!
//! - **Chat**: conversational messages (greetings, questions, opinions, thanks).
//!   The LLM provides a complete response in `chat_text`. One call total.
//! - **Order**: actionable requests (create, search, fix, open, build, etc.).
//!   The LLM provides a brief acknowledgment in `chat_text` and classifies difficulty.
//! - **Stop**: user wants the agent to stop, cancel, or abandon the current task.
//!   Short acknowledgement in `chat_text`. Caller should interrupt any active task.

use serde::{Deserialize, Serialize};
use skyclaw_core::types::error::SkyclawError;
use skyclaw_core::types::message::{ChatMessage, CompletionRequest, ContentPart, Usage};
use skyclaw_core::types::optimization::ExecutionProfile;
use skyclaw_core::Provider;
use tracing::{debug, info, warn};

/// Classification result from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageClassification {
    pub category: MessageCategory,
    pub chat_text: String,
    pub difficulty: TaskDifficulty,
}

/// Whether a message is conversational, an actionable order, or a stop request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageCategory {
    Chat,
    Order,
    Stop,
}

/// Difficulty level for order messages, maps to execution profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskDifficulty {
    Simple,
    Standard,
    Complex,
}

impl TaskDifficulty {
    /// Convert to an execution profile for the agent pipeline.
    pub fn execution_profile(&self) -> ExecutionProfile {
        match self {
            TaskDifficulty::Simple => ExecutionProfile::simple(),
            TaskDifficulty::Standard => ExecutionProfile::standard(),
            TaskDifficulty::Complex => ExecutionProfile::complex(),
        }
    }
}

const CLASSIFY_SYSTEM_PROMPT: &str = r#"You are SkyClaw, an AI assistant. Classify the user's message and respond with ONLY a valid JSON object. No markdown, no explanation — just the JSON.

Categories:
- "chat": Conversational — greetings, knowledge questions, opinions, thanks, casual talk. You provide a complete helpful response.
- "order": The user wants you to DO something — open, create, search, fix, write, build, run, find, download, deploy, browse, etc.
- "stop": The user wants you to STOP, cancel, or abandon the current task. Any variation of "stop", "cancel", "don't continue", "never mind", "forget it", "that's enough", "no need", or equivalent in any language. Even if embedded in a longer sentence like "ok stop that" or "thôi không cần nữa".

Difficulty (for orders only):
- "simple": Single step, straightforward task
- "standard": Multi-step task requiring tools
- "complex": Deep work — debug, architecture, research, multi-tool analysis

Response format:
{"category":"chat","chat_text":"your response","difficulty":"simple"}

Rules:
- For "chat": chat_text = your complete, helpful answer to the user.
- For "order": chat_text = brief natural acknowledgment (1-2 sentences, e.g. "Let me search for that!" or "On it, opening YouTube now.").
- For "stop": chat_text = very short acknowledgment in the user's language (e.g. "OK, stopped." / "Đã dừng." / "了解、中止しました。"). Nothing else.
- difficulty is only meaningful for "order". For "chat" and "stop", always use "simple".
- Respond in the SAME LANGUAGE as the user's message."#;

/// Classify a user message using a fast LLM call.
///
/// `history` must already include the current user message as its last element.
/// Returns the classification and the raw usage for budget tracking.
/// Falls back with an error if the provider call or JSON parsing fails —
/// the caller should use rule-based classification as fallback.
pub async fn classify_message(
    provider: &dyn Provider,
    model: &str,
    _user_text: &str,
    history: &[ChatMessage],
) -> Result<(MessageClassification, Usage), SkyclawError> {
    // Use last 10 history messages for conversational context.
    // History already includes the current user message (pushed by runtime
    // before calling classify), so we don't add it again.
    let context_start = history.len().saturating_sub(10);
    let messages: Vec<ChatMessage> = history[context_start..].to_vec();

    let request = CompletionRequest {
        model: model.to_string(),
        messages,
        tools: vec![],
        max_tokens: Some(1000),
        temperature: Some(0.0),
        system: Some(CLASSIFY_SYSTEM_PROMPT.to_string()),
    };

    debug!("LLM classify: sending classification request");

    let response = provider.complete(request).await?;

    // Extract text from response content
    let response_text = response
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    debug!(raw_response = %response_text, "LLM classify: got response");

    let classification = parse_classification(&response_text)?;

    info!(
        category = ?classification.category,
        difficulty = ?classification.difficulty,
        chat_text_len = classification.chat_text.len(),
        "LLM classify: message classified"
    );

    Ok((classification, response.usage))
}

/// Parse the classification JSON from the LLM response.
/// Handles markdown code blocks, extra whitespace, and surrounding text.
fn parse_classification(text: &str) -> Result<MessageClassification, SkyclawError> {
    let json_str = extract_json(text);

    serde_json::from_str::<MessageClassification>(json_str).map_err(|e| {
        warn!(
            error = %e,
            raw = %text,
            "Failed to parse classification JSON"
        );
        SkyclawError::Provider(format!("Classification parse error: {}", e))
    })
}

/// Extract JSON object from text that may contain markdown formatting
/// or surrounding prose.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();

    // Find the first '{' and last '}' to extract the JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end >= start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chat_classification() {
        let json = r#"{"category":"chat","chat_text":"Hello! How can I help you today?","difficulty":"simple"}"#;
        let result = parse_classification(json).unwrap();
        assert_eq!(result.category, MessageCategory::Chat);
        assert_eq!(result.chat_text, "Hello! How can I help you today?");
        assert_eq!(result.difficulty, TaskDifficulty::Simple);
    }

    #[test]
    fn parse_order_classification() {
        let json = r#"{"category":"order","chat_text":"On it! Let me search for that.","difficulty":"standard"}"#;
        let result = parse_classification(json).unwrap();
        assert_eq!(result.category, MessageCategory::Order);
        assert_eq!(result.chat_text, "On it! Let me search for that.");
        assert_eq!(result.difficulty, TaskDifficulty::Standard);
    }

    #[test]
    fn parse_complex_order() {
        let json = r#"{"category":"order","chat_text":"Let me dig into that codebase.","difficulty":"complex"}"#;
        let result = parse_classification(json).unwrap();
        assert_eq!(result.category, MessageCategory::Order);
        assert_eq!(result.difficulty, TaskDifficulty::Complex);
    }

    #[test]
    fn parse_stop_classification() {
        let json = r#"{"category":"stop","chat_text":"Đã dừng.","difficulty":"simple"}"#;
        let result = parse_classification(json).unwrap();
        assert_eq!(result.category, MessageCategory::Stop);
        assert_eq!(result.chat_text, "Đã dừng.");
        assert_eq!(result.difficulty, TaskDifficulty::Simple);
    }

    #[test]
    fn parse_stop_english() {
        let json = r#"{"category":"stop","chat_text":"OK, stopped.","difficulty":"simple"}"#;
        let result = parse_classification(json).unwrap();
        assert_eq!(result.category, MessageCategory::Stop);
        assert_eq!(result.chat_text, "OK, stopped.");
    }

    #[test]
    fn parse_with_markdown_code_block() {
        let text =
            "```json\n{\"category\":\"chat\",\"chat_text\":\"Hi!\",\"difficulty\":\"simple\"}\n```";
        let result = parse_classification(text).unwrap();
        assert_eq!(result.category, MessageCategory::Chat);
        assert_eq!(result.chat_text, "Hi!");
    }

    #[test]
    fn parse_with_surrounding_text() {
        let text = "Here is the classification: {\"category\":\"order\",\"chat_text\":\"Sure!\",\"difficulty\":\"complex\"} end";
        let result = parse_classification(text).unwrap();
        assert_eq!(result.category, MessageCategory::Order);
        assert_eq!(result.difficulty, TaskDifficulty::Complex);
    }

    #[test]
    fn parse_with_extra_whitespace() {
        let text =
            "  \n  {\"category\":\"chat\",\"chat_text\":\"OK\",\"difficulty\":\"simple\"}  \n  ";
        let result = parse_classification(text).unwrap();
        assert_eq!(result.category, MessageCategory::Chat);
    }

    #[test]
    fn invalid_json_returns_error() {
        let result = parse_classification("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn empty_input_returns_error() {
        let result = parse_classification("");
        assert!(result.is_err());
    }

    #[test]
    fn difficulty_maps_to_execution_profile() {
        let simple = TaskDifficulty::Simple.execution_profile();
        assert_eq!(simple.max_iterations, 2);
        assert!(!simple.skip_tool_loop);

        let standard = TaskDifficulty::Standard.execution_profile();
        assert_eq!(standard.max_iterations, 5);

        let complex = TaskDifficulty::Complex.execution_profile();
        assert_eq!(complex.max_iterations, 10);
    }

    #[test]
    fn category_serde_roundtrip() {
        let chat = MessageCategory::Chat;
        let json = serde_json::to_string(&chat).unwrap();
        assert_eq!(json, "\"chat\"");
        let restored: MessageCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, MessageCategory::Chat);

        let order = MessageCategory::Order;
        let json = serde_json::to_string(&order).unwrap();
        assert_eq!(json, "\"order\"");

        let stop = MessageCategory::Stop;
        let json = serde_json::to_string(&stop).unwrap();
        assert_eq!(json, "\"stop\"");
        let restored: MessageCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, MessageCategory::Stop);
    }

    #[test]
    fn difficulty_serde_roundtrip() {
        for difficulty in [
            TaskDifficulty::Simple,
            TaskDifficulty::Standard,
            TaskDifficulty::Complex,
        ] {
            let json = serde_json::to_string(&difficulty).unwrap();
            let restored: TaskDifficulty = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, difficulty);
        }
    }

    #[test]
    fn full_classification_serde_roundtrip() {
        let classification = MessageClassification {
            category: MessageCategory::Order,
            chat_text: "Looking into it!".to_string(),
            difficulty: TaskDifficulty::Standard,
        };
        let json = serde_json::to_string(&classification).unwrap();
        let restored: MessageClassification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.category, MessageCategory::Order);
        assert_eq!(restored.chat_text, "Looking into it!");
        assert_eq!(restored.difficulty, TaskDifficulty::Standard);
    }
}
