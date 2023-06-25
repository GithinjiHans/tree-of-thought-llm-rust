use async_openai::{
	error::OpenAIError,
	types::{ChatCompletionRequestMessage, ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, CreateChatCompletionResponse, Role},
	Client,
};
use backoff::future::retry;
use backoff::ExponentialBackoff;

static mut COMPLETION_TOKENS: u32 = 0;
static mut PROMPT_TOKENS: u32 = 0;

pub async fn completions_with_backoff(
	model: Option<&str>,
	messages: &Vec<ChatCompletionRequestMessage>,
	temperature: Option<f32>,
	max_tokens: Option<u16>,
	n: Option<isize>,
	stop: Option<&str>,
) -> Result<CreateChatCompletionResponse, OpenAIError> {
	let client = Client::new();
	let mut request_builder = CreateChatCompletionRequestArgs::default();
	request_builder
		.model(model.unwrap_or("gpt-4"))
		.temperature(temperature.unwrap_or(0.7))
		.max_tokens(max_tokens.unwrap_or(1000))
		.n(n.unwrap_or(1) as u8)
		.messages(messages.to_owned());

	stop.map(|stop| request_builder.stop(stop));
	let request = request_builder.build().unwrap();

	retry(ExponentialBackoff::default(), || async {
		println!("Fetching {:?}", model);
		Ok(client.chat().create(request.clone()).await?)
	})
	.await
}

pub async fn gpt(prompt: &str, model: Option<&str>, temperature: Option<f32>, max_tokens: Option<u16>, n: Option<isize>, stop: Option<&str>) -> Vec<String> {
	let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();
	messages.push(ChatCompletionRequestMessageArgs::default().role(Role::User).content(prompt).build().unwrap());
	chatgpt(messages, model.unwrap_or("gpt_4"), temperature.unwrap_or(0.7), max_tokens.unwrap_or(1000), n.unwrap_or(1), stop).await
}

pub async fn chatgpt(messages: Vec<ChatCompletionRequestMessage>, model: &str, temperature: f32, max_tokens: u16, mut n: isize, stop: Option<&str>) -> Vec<String> {
	let mut outputs = Vec::new();
	while n > 0 {
		let cnt = n.min(20);
		n -= cnt;
		let res = completions_with_backoff(Some(model), &messages, Some(temperature), Some(max_tokens), Some(cnt), stop).await.unwrap();

		outputs.extend(res.choices.iter().map(|choice| choice.message.content.to_owned()));

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
				let cost = (COMPLETION_TOKENS / 1000) as f64 * 0.06 + (PROMPT_TOKENS / 1000) as f64 * 0.03;
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
