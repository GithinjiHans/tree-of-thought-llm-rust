mod tasks;
mod models;

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
