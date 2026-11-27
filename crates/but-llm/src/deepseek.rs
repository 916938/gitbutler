use anyhow::Result;
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use but_secret::{Sensitive, secret};
use schemars::{JsonSchema, schema_for};
use serde::de::DeserializeOwned;

use crate::{
    chat::ChatMessage,
    client::LLMClient,
    config::AI_DEEPSEEK_SECRET_HANDLE,
    openai_utils::{OpenAIClientProvider, readable, response_blocking, stream_response_blocking},
};

/// DeepSeek speaks the OpenAI protocol at this base, so the shared
/// `OpenAIClientProvider` plumbing applies unchanged.
const DEEPSEEK_API_BASE_DEFAULT: &str = "https://api.deepseek.com/v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeepSeekConfig {
    pub api_base: String,
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        Self {
            api_base: DEEPSEEK_API_BASE_DEFAULT.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeepSeekProvider {
    model: Option<String>,
    config: DeepSeekConfig,
    api_key: Sensitive<String>,
}

impl DeepSeekProvider {
    pub fn with(config: Option<DeepSeekConfig>, model: Option<String>) -> Option<Self> {
        let config = config.unwrap_or_default();
        let api_key = Self::retrieve_api_key()?;
        Some(Self {
            config,
            model,
            api_key,
        })
    }

    fn retrieve_api_key() -> Option<Sensitive<String>> {
        // Try secret storage first, then fall back to env var
        if let Ok(Some(key)) =
            secret::retrieve(AI_DEEPSEEK_SECRET_HANDLE, secret::Namespace::Global)
            && !key.0.trim().is_empty()
        {
            return Some(key);
        }
        if let Ok(val) = std::env::var("DEEPSEEK_API_KEY")
            && !val.trim().is_empty()
        {
            return Some(Sensitive(val));
        }
        None
    }
}

impl OpenAIClientProvider for DeepSeekProvider {
    fn client(&self) -> Result<Client<OpenAIConfig>> {
        let open_ai_config = OpenAIConfig::new()
            .with_api_base(self.config.api_base.clone())
            .with_api_key(self.api_key.0.clone());

        Ok(Client::with_config(open_ai_config))
    }
}

/// DeepSeek rejects `response_format: json_schema` ("This response_format type is
/// unavailable now") and only offers JSON mode, which guarantees valid JSON but not its
/// shape. The schema rides along in the system message instead, and the reply is still
/// deserialized as `T`.
fn structured_request<T: JsonSchema>(
    system_message: &str,
    chat_messages: Vec<ChatMessage>,
    model: &str,
) -> Result<CreateChatCompletionRequest> {
    let schema = serde_json::to_string_pretty(&schema_for!(T))?;
    let system_message = format!(
        "{system_message}\n\nAnswer with one JSON object that validates against this JSON \
         schema, and nothing else:\n{schema}"
    );
    let mut messages: Vec<ChatCompletionRequestMessage> =
        vec![ChatCompletionRequestSystemMessage::from(system_message).into()];
    messages.extend(
        chat_messages
            .into_iter()
            .map(ChatCompletionRequestMessage::from),
    );

    CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages(messages)
        .response_format(ResponseFormat::JsonObject)
        .build()
        .map_err(readable)
}

async fn structured_response<T: DeserializeOwned>(
    client: Client<OpenAIConfig>,
    request: CreateChatCompletionRequest,
) -> Result<Option<T>> {
    let response = client.chat().create(request).await.map_err(readable)?;

    for choice in response.choices {
        if let Some(content) = choice.message.content {
            return Ok(Some(serde_json::from_str(&content)?));
        }
    }

    Ok(None)
}

fn structured_output_via_json_object<T: JsonSchema + DeserializeOwned + Send + 'static>(
    provider: &DeepSeekProvider,
    system_message: &str,
    chat_messages: Vec<ChatMessage>,
    model: &str,
) -> Result<Option<T>> {
    let request = structured_request::<T>(system_message, chat_messages, model)?;
    let client = provider.client()?;

    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(structured_response::<T>(client, request))
    })
    .join()
    .unwrap()
}

impl LLMClient for DeepSeekProvider {
    fn model(&self) -> Option<String> {
        self.model.clone()
    }

    fn stream_response(
        &self,
        system_message: &str,
        chat_messages: Vec<ChatMessage>,
        model: &str,
        on_token: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<Option<String>> {
        stream_response_blocking(self, system_message, chat_messages, model, on_token)
    }

    fn structured_output<
        T: serde::Serialize + DeserializeOwned + JsonSchema + std::marker::Send + 'static,
    >(
        &self,
        system_message: &str,
        chat_messages: Vec<ChatMessage>,
        model: &str,
    ) -> Result<Option<T>> {
        structured_output_via_json_object(self, system_message, chat_messages, model)
    }

    fn response(
        &self,
        system_message: &str,
        chat_messages: Vec<ChatMessage>,
        model: &str,
    ) -> Result<Option<String>> {
        response_blocking(self, system_message, chat_messages, model)
    }
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::structured_request;
    use crate::ChatMessage;

    #[derive(JsonSchema, Deserialize)]
    #[allow(dead_code)]
    struct Answer {
        resolution: String,
    }

    #[test]
    fn asks_for_a_json_object_and_carries_the_schema_in_the_prompt() {
        let request = structured_request::<Answer>(
            "Resolve the hunk.",
            vec![ChatMessage::User("the conflicting lines".into())],
            "deepseek-v4-pro",
        )
        .unwrap();
        let request = serde_json::to_value(&request).unwrap();

        // DeepSeek answers `json_schema` with "This response_format type is unavailable now".
        assert_eq!(
            request["response_format"],
            serde_json::json!({ "type": "json_object" }),
            "JSON mode is the only structured format DeepSeek accepts"
        );
        let system = request["messages"][0]["content"].as_str().unwrap();
        assert!(
            system.starts_with("Resolve the hunk."),
            "the caller's instructions come first"
        );
        assert!(
            system.contains("\"resolution\""),
            "without schema enforcement, the model only sees the shape if the prompt shows it"
        );
    }
}
