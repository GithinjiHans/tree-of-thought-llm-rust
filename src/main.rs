use crate::models::gpt;

mod models;
mod tasks;

struct Opts {
	backend: String,
	temperature: f64,

	task: String,
	task_file_path: String,
	task_start_index: isize,
	task_end_index: isize,

	naive_run: bool,
	prompt_sample: Option<String>,

	method_generate: Option<String>,
	method_evaluate: Option<String>,
	method_select: Option<String>,

	n_generate_sample: isize,
	n_evaluate_sample: isize,
	n_select_sample: isize,
}

fn get_current_number(y: &str) -> Option<&str> {
	y.trim().lines().last().unwrap_or("").split("left: ").last().unwrap_or("").split(')').next()
}

static VALUE_LAST_STEP_PROMPT: &'static str = r#"Use numbers and basic arithmetic operations (+ - * /) to obtain 24. Given an input and an answer, give a judgement (sure/impossible) if the answer is correct, i.e. it uses each input exactly once and no other numbers, and reach 24.
Input: 4 4 6 8
Answer: (4 + 8) * (6 - 4) = 24
Judge:
sure
Input: 2 9 10 12
Answer: 2 * 12 * (10 - 9) = 24
Judge:
sure
Input: 4 9 10 13
Answer: (13 - 9) * (10 - 4) = 24
Judge:
sure
Input: 4 4 6 8
Answer: (4 + 8) * (6 - 4) + 1 = 25
Judge:
impossible
Input: 2 9 10 12
Answer: 2 * (12 - 10) = 24
Judge:
impossible
Input: 4 9 10 13
Answer: (13 - 4) * (10 - 9) = 24
Judge:
impossible
Input: {input}
Answer: {ans}
Judge:"#;

static VALUE_PROMPT: &'static str = r#"Evaluate if given numbers can reach 24 (sure/likely/impossible)
10 14
10 + 14 = 24
sure
11 12
11 + 12 = 23
12 - 11 = 1
11 * 12 = 132
11 / 12 = 0.91
impossible
4 4 10
4 + 4 + 10 = 8 + 10 = 18
4 * 10 - 4 = 40 - 4 = 36
(10 - 4) * 4 = 6 * 4 = 24
sure
4 9 11
9 + 11 + 4 = 20 + 4 = 24
sure
5 7 8
5 + 7 + 8 = 12 + 8 = 20
(8 - 5) * 7 = 3 * 7 = 21
I cannot obtain 24 now, but numbers are within a reasonable range
likely
5 6 6
5 + 6 + 6 = 17
(6 - 5) * 6 = 1 * 6 = 6
I cannot obtain 24 now, but numbers are within a reasonable range
likely
10 10 11
10 + 10 + 11 = 31
(11 - 10) * 10 = 10
10 10 10 are all too big
impossible
1 3 3
1 * 3 * 3 = 9
(1 + 3) * 3 = 12
1 3 3 are all too small
impossible
{input}
"#;

async fn get_value<'task>(task: &'task tasks::Task, x: &str, y: &str, n_evaluate_sample: u8, cache_value: bool) -> anyhow::Result<&'task String> {
	match task {
		tasks::Task::Game24 { value_cache, .. } => {
			let last_line = y.trim().lines().last().unwrap_or("");
			let value_prompt = if !last_line.contains("left: ") {
				let ans = last_line.to_lowercase().replace("answer: ", "");
				VALUE_LAST_STEP_PROMPT.replace("{input}", x).replace("{ans}", &ans)
			} else {
				let current_numbers = get_current_number(y).unwrap();
				VALUE_PROMPT.replace("{input}", current_numbers)
			};

			if cache_value && value_cache.contains_key(&value_prompt) {
				return value_cache.get(&value_prompt).ok_or(anyhow::anyhow!("Value not found in cache"));
			} else {
				let outputs = gpt(&value_prompt, None, None, None, Some(n_evaluate_sample), None).await;
				todo!()
			}
		}
		task => anyhow::bail!("Invalid Task: {task:?}"),
	}
}

fn get_values(task: &tasks::Task, x: &str, y: &str, n_evaluate_sample: i32, cache_value: bool) -> anyhow::Result<String> {
	match task {
		tasks::Task::Game24 { data, stops, steps, value_cache } => {
			let last_line = y.trim().lines().last().unwrap_or("");
			if !last_line.contains("left: ") {
				let ans = last_line.to_lowercase().replace("answer: ", "");
				let res = VALUE_LAST_STEP_PROMPT.replace("{input}", x).replace("{ans}", &ans);
				Ok(res)
			} else {
				let current_numbers = get_current_number(y).unwrap();
				Ok(VALUE_PROMPT.replace("{input}", current_numbers))
			}
		}
		task => anyhow::bail!("Invalid Task: {task:?}"),
	}
}

fn parse_args() -> anyhow::Result<Opts> {
	let mut args = pico_args::Arguments::from_env();

	let backend = args.opt_value_from_str("--backend")?;
	let backend = backend.unwrap_or_else(|| "gpt-4".to_string());

	match backend.as_str() {
		"gpt-4" | "gpt-3.5-turbo" => {}
		_ => anyhow::bail!("Invalid backend: {}", backend),
	}

	let temperature = args.opt_value_from_str("--temperature")?.unwrap_or(0.7f64);

	let task = args.value_from_str("--task")?;
	let task_file_path = args.value_from_str("--task_file_path")?;

	let task_start_index = args.opt_value_from_str("--task_start_index")?.unwrap_or(900isize);
	let task_end_index = args.opt_value_from_str("--task_end_index")?.unwrap_or(1000isize);

	let naive_run = args.contains("--naive_run");

	let prompt_sample: Option<String> = args.opt_value_from_str("--prompt_sample")?;
	match prompt_sample.as_ref().map(|s| s.as_str()) {
		Some("standard" | "cot") | None => {}
		sample => anyhow::bail!("Invalid prompt_sample: {:?}", sample),
	}

	let method_generate: Option<String> = args.opt_value_from_str("--method_generate")?;
	match method_generate.as_ref().map(|s| s.as_str()) {
		Some("sample" | "propose") | None => {}
		sample => anyhow::bail!("Invalid method_generate: {:?}", sample),
	}

	let method_evaluate: Option<String> = args.opt_value_from_str("--method_evaluate")?;
	match method_evaluate.as_ref().map(|s| s.as_str()) {
		Some("value" | "vote") | None => {}
		sample => anyhow::bail!("Invalid method_evaluate: {:?}", sample),
	}

	let method_select: Option<String> = args.opt_value_from_str("--method_select")?;
	match method_select.as_ref().map(|s| s.as_str()) {
		Some("sample" | "greedy") | None => {}
		sample => anyhow::bail!("Invalid method_select: {:?}", sample),
	}

	let n_generate_sample = args.opt_value_from_str("--n_generate_sample")?.unwrap_or(1isize);
	let n_evaluate_sample = args.opt_value_from_str("--n_evaluate_sample")?.unwrap_or(1isize);
	let n_select_sample = args.opt_value_from_str("--n_select_sample")?.unwrap_or(1isize);

	Ok(Opts {
		backend,
		temperature,
		task,
		task_file_path,
		task_start_index,
		task_end_index,
		naive_run,
		prompt_sample,
		method_generate,
		method_evaluate,
		method_select,
		n_generate_sample,
		n_evaluate_sample,
		n_select_sample,
	})
}

fn main() -> anyhow::Result<()> {
	let options = parse_args()?;
	let task = tasks::get_task(&options.task, &options.task_file_path)?;

	Ok(())
}
