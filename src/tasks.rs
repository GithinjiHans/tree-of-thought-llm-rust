use anyhow::Ok;

use crate::{
	models::gpt,
	strings::{self, SCORE_PROMPT_TEXT, VOTE_PROMPT_TEXT},
};
use regex::Regex;
use std::{
	collections::BTreeMap,
	path::Path,
};

pub const DATA_PATH: &str = "./data";

#[derive(Debug,Clone)]
pub(crate) enum Task {
	Game24 {
		data: Vec<String>,
		stops: [char; 4],
		steps: isize,
		value_cache: BTreeMap<String, f32>,
	},
	Text {
		data: Vec<String>,
		stops: [Option<&'static str>; 2],
		steps: isize,
	},
	MiniCrossword {
		env: MiniCrosswordEnv,
		xs: Vec<String>,
		steps: isize,
		cache_proposals: (),
	},
}

#[derive(Debug,Clone,serde::Serialize)]

pub struct TOutput{
r_letter:f32,
r_word:f32,
r_game:bool,
pub r:f32,
rs:Vec<isize>,
}

impl TOutput{
	pub fn new()->TOutput{
      TOutput { r_letter: 0.0, r_word: 0.0, r_game: false, r: 0.0, rs: vec![] }
	}
}

fn get_current_number(y: &str) -> Option<&str> {
	y.trim().lines().last().unwrap_or("").split("left: ").last().unwrap_or("").split(')').next()
}

impl Task {
	pub fn get_steps(&self) -> isize {
		match self {
			Task::Game24 { steps, .. } | Task::Text { steps, .. } | Task::MiniCrossword { steps, .. } => *steps,
		}
	}

	pub fn get_input(&mut self, idx: usize) -> anyhow::Result<String> {
		match self {
			Task::MiniCrossword { env, .. } => {
				env.reset(idx)?;
				Ok(env.render_clues(None))
			}
			Task::Text { data, .. } | Task::Game24 { data, .. } => data.get(idx).ok_or(anyhow::anyhow!("Item not found")).cloned(),
		}
	}

	async fn get_value(&mut self, x: &str, y: &str, model: Option<&str>, n_evaluate_sample: isize, cache_value: bool) -> anyhow::Result<f32> {
		match self {
			Task::Game24 { ref mut value_cache, .. } => {
				let last_line = y.trim().lines().last().unwrap_or("");
				let value_prompt = if !last_line.contains("left: ") {
					let ans = last_line.to_lowercase().replace("answer: ", "");
					strings::VALUE_LAST_STEP_PROMPT.replace("{input}", x).replace("{ans}", &ans)
				} else {
					let current_numbers = get_current_number(y).unwrap();
					strings::VALUE_PROMPT_GAME24.replace("{input}", current_numbers)
				};

				if cache_value && value_cache.contains_key(&value_prompt) {
					return value_cache.get(&value_prompt).ok_or(anyhow::anyhow!("Value not found in cache")).cloned();
				} else {
					let outputs = gpt(&value_prompt, model, None, None, Some(n_evaluate_sample), None).await;
					let value = if y.trim().lines().count() == 4 && !y.to_lowercase().contains("answer") {
						0f32
					} else {
						let value_names = outputs.iter().map(|v| v.lines().last().unwrap_or("")).collect::<Vec<_>>();
						[("sure", 20f32), ("likely", 1f32), ("impossible", 0.001f32)]
							.iter()
							.map(|(name, value)| value * value_names.iter().filter(|n| *n == name).count() as f32)
							.sum()
					};

					if cache_value {
						value_cache.insert(value_prompt, value);
					}

					Ok(value)
				}
			}
			task => anyhow::bail!("Invalid Task: {task:?}"),
		}
	}

	pub async fn get_values(&mut self, x: &str, ys: &[String], model: Option<&str>, n_evaluate_sample: isize, cache_value: Option<bool>) -> anyhow::Result<Vec<f32>> {
		let mut values = vec![];
		let mut local_value_cache = BTreeMap::new();
        let mut  value;
		for y in ys {
			if local_value_cache.contains_key(y) {
				value=0f32;
			} else {
				value = self.get_value(x, y, model, n_evaluate_sample, cache_value.unwrap_or(true)).await?;
				local_value_cache.insert(y.to_string(), value);
			}
			values.push(value);
		}

		Ok(values)
	}

	pub async fn get_samples(&self, x: &str, y: &str, model: Option<&str>, n_generate_sample: isize, prompt_sample: &str, stop: Option<&str>) -> anyhow::Result<Vec<String>> {
		let prompt = match prompt_sample {
			"standard" => self.standard_prompt_wrap(x, y),
			"cot" => self.cot_prompt_wrap(x, y),
			sample => anyhow::bail!("Prompt sample {} not recognized", sample),
		};

		let samples = gpt(&prompt, model, None, None, Some(n_generate_sample), stop).await;
		Ok(samples.iter().map(|s| format!("{y}{s}")).collect())
	}

	pub async fn get_votes(&self, _x: &str, ys: &Vec<String>, n_evaluate_sample: isize) -> anyhow::Result<Vec<f32>> {
		let vote_prompt = self.vote_prompt_wrap( ys);
		let vote_outputs = gpt(&vote_prompt, Some("gpt-3.5-turbo"), None, None, Some(n_evaluate_sample), None).await;
		let values = self.vote_outputs_unwrap(&vote_outputs, ys.len());
		Ok(values)
	}
	pub async fn get_proposals(&mut self, x: &str, y: &str, model: Option<&str>) -> anyhow::Result<Vec<String>> {
		let propose_prompt = self.propose_prompt_wrap(x, y)?;
		let output = gpt(&propose_prompt, model, None, None, Some(1), None).await;
		let Some(outputs) = output.first() else {
			anyhow::bail!("No outputs found");
		};
		Ok(outputs.lines().map(|o| format!("{}{}\n", y, o)).collect::<Vec<_>>())
	}
	pub fn set_status(&mut self, x: &str, y: &str) -> anyhow::Result<TOutput> {
		match self {
			Task::MiniCrossword { env, xs, .. } => {
				let Some((idx, _)) = xs.iter().enumerate().find(|(_, val)| val.as_str() == x) else {
					anyhow::bail!("Item not found");
				};
				// MiniCrossword test output
				env.reset(idx)?;
				let Some(output) = y.split("Output:\n").last() else {
					anyhow::bail!("Y is empty or does not contain Output:\n");
				};

				let skip = output.trim().lines().count() - 4;
				let mut info = TOutput::new();
				for (i, line) in output.trim().lines().skip(skip).enumerate() {
					let word = line.split(' ').take(5).collect::<String>();
					let repeat = 5 - word.chars().count();
					let word = (word + " ").repeat(repeat);
					let action = format!("h{i}. {word}");
					info = env.step(&action).expect("").letter;
				}
				Ok(info)
			}
			Task::Text { .. } | Task::Game24 { .. } => anyhow::bail!("Set status not implemented for Text and Game24"),
		}
	}

	pub fn propose_prompt_wrap(&mut self, x: &str, y: &str) -> anyhow::Result<String> {
		let task = self as *mut Task;
		match self {
			Task::MiniCrossword { env, .. } => {
				unsafe { task.as_mut().unwrap() }.set_status(x, y)?;
				Ok(strings::PROPOSE_PROMPT_CROSSWORDS.replace("{input}", env.render(None).as_str()))
			}
			Task::Game24 { .. } => {
				let input = if !y.is_empty() { y } else { x };
				let current_numbers = get_current_number(input);
				let prompt = if matches!(current_numbers, Some("24")) {
					strings::COT_PROMPT_GAME24.replace("{input}", x) + "Steps:" + y
				} else {
					strings::PROPOSE_PROMPT_GAME24.replace("{input}", current_numbers.unwrap_or(""))
				};

				Ok(prompt)
			}
			Task::Text { .. } => anyhow::bail!("Propose prompt not implemented for Text"),
		}
	}
	pub fn standard_prompt_wrap(&self, x: &str, y: &str) -> String {
		match self {
			Task::MiniCrossword { .. } => {
				let mut prompt = strings::STANDARD_PROMPT_CROSSWORDS.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
			Task::Game24 { .. } => {
				let mut prompt = strings::STANDARD_PROMPT_GAME24.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
			Task::Text { .. } => {
				let mut prompt = strings::STANDARD_PROMPT_TEXT.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
		}
	}
	pub async fn test_output(self, idx: isize, output: &str) -> anyhow::Result<TOutput> {
		match self {
			Task::Game24 { data, .. } => {
				let mut result = TOutput::new();
				let expression = output.trim().split('\n').last().unwrap().to_lowercase().replace("answer: ", "").split('=').next().unwrap().to_string();

				let numbers_regex = Regex::new(r"\d+").unwrap();
				let mut numbers: Vec<String> = numbers_regex.find_iter(&expression).map(|m| m.as_str().to_string()).collect();

				let mut problem_numbers: Vec<String> = numbers_regex.find_iter(data.get(idx as usize).unwrap()).map(|m| m.as_str().to_string()).collect();
				if numbers.sort() != problem_numbers.sort() {
					result.r=0.0;
					Ok(result)
				}
				// else if  {
				// 	todo!()
				// }
				else {
					result.r = 0.0;
					Ok(result)
				}
			}
			Task::Text { .. } => {
				let output = output.split("Passage:\n").last().unwrap_or("");
				let mut info = TOutput::new();
				let prompt = SCORE_PROMPT_TEXT.to_owned() + output;
				let score_outputs = gpt(&prompt, Some("gpt-3.5-turbo"), None, None, None, None).await;
				let mut scores: Vec<isize> = vec![];
				let pattern = Regex::new(r".*coherency score is (\d+).*").unwrap();
				for score_output in score_outputs {
					if let Some(captures) = pattern.captures(&score_output) {
						if let Some(score) = captures.get(1).and_then(|m| m.as_str().parse::<isize>().ok()) {
							scores.push(score);
						} else {
							println!("------------------score no match: {}", score_output);
						}
					}
				}
				println!("{:?}", scores);
				info.rs = scores.clone();
                info.r= if scores.is_empty() { 0.0 } else {scores.iter().sum::<isize>() as f32/ scores.len() as f32};
				Ok(info)
			}
			Task::MiniCrossword { mut env, .. } => {
				env.reset(idx.try_into().expect("failed to convert isize to usize")).expect("failed to reset");
				let output = output.split("Output:\n").last().unwrap_or("");
				let mut info = TOutput::new();
				//  todo (For loop)
				let lines: Vec<&str> = output.trim().split('\n').collect();
				let start_index = if lines.len() >= 5 { lines.len() - 5 } else { 0 };
				let last_five_lines: Vec<&str> = lines[start_index..].to_vec();
				for (i, line) in last_five_lines.iter().enumerate() {
					let letters = line.split(' ').take(5).collect::<String>();
					let word = letters;
					let word_padded = format!("{}{}", word, "_".repeat(5 - word.len()));
					let action = format!("h{}. {}", i, word_padded);
					// println!("{}", action);
					info = env.step(&action).expect("").letter;
				}

				info.r = info.r_word;
				Ok(info)
			}
		}
	}
	pub fn cot_prompt_wrap(&self, x: &str, y: &str) -> String {
		match self {
			Task::MiniCrossword { .. } => {
				let mut prompt = strings::COT_PROMPT_CROSSWORDS.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
			Task::Game24 { .. } => {
				let mut prompt = strings::COT_PROMPT_GAME24.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
			Task::Text { .. } => {
				let mut prompt = strings::COT_PROMPT_TEXT.replace("{input}", x);
				prompt.push_str(y);
				prompt
			}
		}
	}
	pub fn vote_prompt_wrap(&self, ys: &Vec<String>) -> String {
		let mut prompt = VOTE_PROMPT_TEXT.to_owned();
		for (i, y) in ys.iter().enumerate() {
			let choice_prompt = format!("Choice {}:\n{}\n", i + 1, y);
			prompt += &choice_prompt;
		}
		return prompt;
	}

	fn vote_outputs_unwrap(&self, vote_outputs: &[String], n_candidates: usize) -> Vec<f32> {
		let mut vote_results = vec![0.0; n_candidates];

		let pattern = Regex::new(r".*best choice is .*(\d+).*").unwrap();

		for vote_output in vote_outputs {
			if let Some(capture) = pattern.captures(vote_output) {
				if let Some(vote) = capture.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) {
					if vote < n_candidates {
						vote_results[vote] += 1.0;
					}
				}
			} else {
				println!("vote no match: {}", vote_output);
			}
		}

		vote_results
	}
}
pub struct Out {
	render: String,
	r_all: bool,
	all: bool,
	letter: TOutput,
}
#[derive(Debug,Clone)]
pub struct MiniCrosswordEnv {
	file: Vec<serde_json::Value>,
	n: usize,
	idx: Option<usize>,
	times: usize,

	cache: (),
	prompt_status_cache: (),
	ext: MiniCrosswordEnvExt,
}

impl MiniCrosswordEnv {
	fn reset(&mut self, idx: usize) -> anyhow::Result<String> {
		self.idx = Some(idx);

		let base = &self.file[idx];
		self.ext.data = base[0].clone();
		self.ext.board_gt = serde_json::from_value(base[1].clone())?;

		self.ext.board = vec!['_'.into(); 25];
		self.ext.ans = vec!["_____".into(); 10];
		self.ext.ans_gt = self.get_ans(&self.ext.board_gt);

		self.ext.steps = 0;
		self.ext.status = vec![0; 10];

		Ok(self.render(None))
	}

	fn render(&self, status: Option<bool>) -> String {
		let mut s = self.render_board();

		if status.unwrap_or(false) {
			s.push_str("\nUnfilled:\n");
			s.push_str(&self.render_ans(Some(0)));
			s.push_str("\nFilled:\n");
			s.push_str(&self.render_ans(Some(1)));
			s.push_str("\nChanged:\n");
			s.push_str(&self.render_ans(Some(2)));
			s
		} else {
			s.push('\n');
			s.push_str(&self.render_ans(None));
			s
		}
	}

	fn render_ans(&self, status: Option<isize>) -> String {
		let horizontal = (0..5usize).fold(String::new(), |mut acc, i| {
			if let Some(s) = status {
				if self.ext.status[i] == s {
					acc.push_str(format!("h{}. {}: {}\n", i + 1, self.ext.data[i], self.ext.ans[i]).as_str());
				}
			}
			acc
		});
		(5..10usize).fold(horizontal, |mut acc, i| {
			if let Some(s) = status {
				if self.ext.status[i] == s {
					acc.push_str(format!("v{}. {}: {}\n", i - 4, self.ext.data[i], self.ext.ans[i]).as_str());
				}
			}
			acc
		})
	}

	fn render_board(&self) -> String {
		(0..5).fold("Current board:\n".to_string(), |acc, next| acc + &self.ext.board[next * 5..(next + 1) * 5].join("") + "\n")
	}

	fn render_clues(&self, status: Option<isize>) -> String {
		let horizontal = (0..5usize).fold(String::new(), |mut acc, i| {
			if let Some(s) = status {
				if self.ext.status[i] == s {
					acc.push_str(format!("h{}. {}\n", i + 1, self.ext.data[i]).as_str());
				}
			}
			acc
		});
		(5..10usize).fold(horizontal, |mut acc, i| {
			if let Some(s) = status {
				if self.ext.status[i] == s {
					acc.push_str(format!("v{}. {}\n", i - 4, self.ext.data[i]).as_str());
				}
			}
			acc
		})
	}

	fn get_ans(&self, board: &Vec<String>) -> Vec<String> {
		let mut ans = vec![String::new(); 10];
		(0..5).for_each(|i| ans[i] = board[i * 5..(i + 1) * 5].join(""));
		(0..5).for_each(|i| ans[i + 5] = board[i..].iter().enumerate().filter(|(idx, _)| (idx - i) % 5 == 0).map(|(_, s)| s.as_str()).collect::<String>());

		ans
	}

	fn step(&mut self, action: &str) -> anyhow::Result<Out> {
		let mut action_parts = action.trim().split('\n').last().expect("Invalid! Format ").split(". ");
		let pos = action_parts.next();
		let word = action_parts.next();

		if pos.is_none() || word.is_none() {
			anyhow::bail!("Invalid! Format should be like \"h1. apple\"")
		}

		let pos = pos.unwrap();
		let word = Box::new(word.unwrap().to_owned());
		if word.len() != 5 {
			anyhow::bail!("Invalid! Word should have 5 letters.")
		}
		let mut idx;
		if pos.starts_with('h') {
			idx = pos[1..].parse::<usize>().unwrap() - 1;
			self.ext.board[idx * 5..(idx + 1) * 5].clone_from_slice(&word.chars().map(|c| c.to_string().to_uppercase()).collect::<Vec<_>>());
		} else if pos.starts_with('v') {
			idx = pos[1..].parse::<usize>().unwrap() - 1;
			self.ext.board[idx..5].clone_from_slice(word.chars().map(|c| c.to_string().to_uppercase()).collect::<Vec<_>>().as_slice());
			idx += 5; // for later status update
		} else {
			anyhow::bail!("Invalid! Position should be h1-h5 or v1-v5")
		}
		self.ext.new_ans = self.get_ans(&self.ext.board);
		self.ext.status = self
			.ext
			.status
			.iter()
			.zip(self.ext.ans.iter().zip(self.ext.new_ans.iter()))
			.map(|(status, (letter, new_letter))| if letter != new_letter && *letter != "_" { 2 } else { *status })
			.collect::<Vec<_>>();
		self.ext.status[idx] = 1;
		self.ext.ans = self.ext.new_ans.clone();
		let r_all = self.ext.board == self.ext.board_gt;
		let test = Out {
			render: self.render(Some(true)),
			r_all,
			all: r_all || self.ext.steps >= 20,
			letter: TOutput { r_letter:  (self.ext.board.iter().zip(self.ext.board_gt.iter()).filter(|y| y.0 == y.1).count() / 25) as f32,
				 r_word: (self.ext.ans.iter().zip(self.ext.ans_gt.iter()).filter(|y| y.0 == y.1).count() / 25) as f32,
				  r_game: r_all,
				   r: 0.0,
				    rs: vec![] 
				},
		};
		Ok(test)
	}
}

#[derive(Default, Debug,Clone)]
pub struct MiniCrosswordEnvExt {
	data: serde_json::Value,
	board_gt: Vec<String>,
	board: Vec<String>,
	ans: Vec<String>,
	new_ans: Vec<String>,
	ans_gt: Vec<String>,
	steps: isize,
	status: Vec<isize>,
}

impl MiniCrosswordEnv {
	fn new(file: Option<&str>) -> anyhow::Result<MiniCrosswordEnv> {
		let path = file.unwrap_or("mini0505.json");
		let path = Path::new(DATA_PATH).join("crosswords").join(path);

		let data = std::fs::read_to_string(path)?;
		let data: serde_json::Value = serde_json::from_str(&data)?;
		let data = data.as_array().ok_or(anyhow::anyhow!("Invalid JSON file, expected Array base structure"))?.clone();

		let n = data.len();

		Ok(MiniCrosswordEnv {
			file: data,
			n,
			idx: None,
			times: 0,
			cache: (),
			prompt_status_cache: (),
			ext: Default::default(),
		})
	}
}

pub(crate) fn get_task(name: &str, file_path: &str) -> anyhow::Result<Task> {
	let task = match name {
		"game24" => {
			let path = Path::new(DATA_PATH).join("24").join(file_path);
			let mut reader = csv::ReaderBuilder::new().has_headers(true).from_path(path)?;

			let mut puzzles = Vec::new();
			for record in reader.records() {
				let record = record?;
				puzzles.push(record.get(1).unwrap().to_string());
			}

			Task::Game24 {
				data: puzzles,
				stops: ['\n', '\n', '\n', '\n'],
				steps: 4,
				value_cache: BTreeMap::new(),
			}
		}
		"text" => {
			let path = Path::new(DATA_PATH).join("text").join(file_path);
			let lines = std::fs::read_to_string(path)?.lines().map(|s| s.to_string()).collect::<Vec<_>>();

			Task::Text {
				data: lines,
				stops: [Some("\nPassage:\n"), None],
				steps: 2,
			}
		}
		"crosswords" => {
			let mut env = MiniCrosswordEnv::new(Some(file_path))?;
			let mut xs = vec![];

			for idx in 0..env.n {
				env.reset(idx)?;
				xs.push(env.render_clues(None));
			}

			Task::MiniCrossword {
				env,
				xs,
				steps: 10,
				cache_proposals: (),
			}
		}
		name => anyhow::bail!("Invalid task: {:?}", name),
	};

	Ok(task)
}
