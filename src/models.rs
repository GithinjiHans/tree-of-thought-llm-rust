    use async_openai::{
        error::OpenAIError,
        types::{
            ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs,
            CreateChatCompletionRequestArgs, CreateChatCompletionResponse, Role,
        },
        Client,
    };
    use backoff::future::retry;
    use backoff::ExponentialBackoff;
    use dotenv::dotenv;
    use std::env;

    static mut COMPLETION_TOKENS: u32 = 0;
    static mut PROMPT_TOKENS: u32 = 0;

    pub async fn completions_with_backoff(
        model: &str,
        messages: &Vec<ChatCompletionRequestMessage>,
        temperature: f32,
        max_tokens: u16,
        n: u8,
        stop: &str,
    ) -> Result<CreateChatCompletionResponse, OpenAIError> {
        dotenv().ok();
        let api_key =
            env::var("OPENAI_API_KEY").expect("API key not found in environment variable");
        let client = Client::new().with_api_key(api_key);
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .temperature(temperature)
            .max_tokens(max_tokens)
            .n(n)
            .messages(messages.to_owned())
            .stop(stop)
            .build()
            .unwrap();
        retry(ExponentialBackoff::default(), || async {
            println!("Fetching {}", model);
            Ok(client.chat().create(request.clone()).await?)
        })
        .await
    }

    pub async fn gpt(
        prompt: String,
        model: &str,
        temperature: f32,
        max_tokens: u16,
        n: u8,
        stop: &str,
    ) -> Vec<String> {
        let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();
        messages.push(
            ChatCompletionRequestMessageArgs::default()
                .role(Role::User)
                .content(prompt)
                .build()
                .unwrap(),
        );
        chatgpt(messages, model, temperature, max_tokens, n, stop).await
    }

    pub async fn chatgpt(
        messages: Vec<ChatCompletionRequestMessage>,
        model: &str,
        temperature: f32,
        max_tokens: u16,
        mut n: u8,
        stop: &str,
    ) -> Vec<String> {
        let mut outputs = Vec::new();
        while n > 0 {
            let cnt = n.min(20);
            n -= cnt;
            let res =
                completions_with_backoff(model, &messages, temperature, max_tokens, cnt, &stop)
                    .await
                    .unwrap();

            outputs.extend(
                res.choices
                    .iter()
                    .map(|choice| choice.message.content.to_owned()),
            );

            // log completion tokens
            unsafe {
                COMPLETION_TOKENS += res.usage.clone().unwrap().completion_tokens;
                PROMPT_TOKENS += res.usage.unwrap().prompt_tokens;
            }
        }
        outputs
    }

    pub async fn gpt_usage(backend: &str) -> (u32, u32, f64) {
        unsafe {
            match backend {
                "gpt-4" => {
                    let cost = (COMPLETION_TOKENS / 1000) as f64 * 0.06
                        + (PROMPT_TOKENS / 1000) as f64 * 0.03;
                    (COMPLETION_TOKENS, PROMPT_TOKENS, cost)
                }
                "gpt-3.5-turbo" => {
                    let cost = ((COMPLETION_TOKENS + PROMPT_TOKENS) / 1000) as f64 * 0.0002;
                    (COMPLETION_TOKENS, PROMPT_TOKENS, cost)
                }
                _ => panic!("Invalid backend"),
            }
        }
    }