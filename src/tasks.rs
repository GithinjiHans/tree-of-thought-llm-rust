use crate::models::gpt;
use std::{collections::BTreeMap, path::Path};

pub const DATA_PATH: &str = "./data";

#[derive(Debug)]
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

	async fn get_value(&mut self, x: &str, y: &str, n_evaluate_sample: u8, cache_value: bool) -> anyhow::Result<f32> {
		match self {
			Task::Game24 { ref mut value_cache, .. } => {
				let last_line = y.trim().lines().last().unwrap_or("");
				let value_prompt = if !last_line.contains("left: ") {
					let ans = last_line.to_lowercase().replace("answer: ", "");
					VALUE_LAST_STEP_PROMPT.replace("{input}", x).replace("{ans}", &ans)
				} else {
					let current_numbers = get_current_number(y).unwrap();
					VALUE_PROMPT.replace("{input}", current_numbers)
				};

				if cache_value && value_cache.contains_key(&value_prompt) {
					return value_cache.get(&value_prompt).ok_or(anyhow::anyhow!("Value not found in cache")).cloned();
				} else {
					let outputs = gpt(&value_prompt, None, None, None, Some(n_evaluate_sample), None).await;
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

	async fn get_values(&mut self, x: &str, ys: &[&str], n_evaluate_sample: u8, cache_value: bool) -> anyhow::Result<Vec<f32>> {
		let mut values = vec![];
		let mut local_value_cache = BTreeMap::new();

		for y in ys {
			if local_value_cache.contains_key(*y) {
				values.push(0f32);
			} else {
				let value = self.get_value(x, y, n_evaluate_sample, cache_value).await?;
				local_value_cache.insert(y.to_string(), value);
				values.push(value);
			}
		}

		Ok(values)
	}

	pub fn get_votes(&self, x: &str, ys: &str, n_evaluate_sample: isize) {}
	pub fn get_proposals(&self, x: &str, y: &str) -> Vec<String> {
		let propose_prompt = {
		};
		todo!()
	}
	pub fn get_samples(&self, x: &str, y: &str, n_evaluate_sample: isize, prompt_sample: &str, stop: Option<&str>) {}
}

#[derive(Debug)]
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

	fn get_ans(&self, board: &[String]) -> Vec<String> {
		let mut ans = vec![String::new(); 10];
		(0..5).for_each(|i| ans[i] = board[i * 5..(i + 1) * 5].join(""));
		(0..5).for_each(|i| ans[i + 5] = board[i..].iter().enumerate().filter(|(idx, _)| (idx - i) % 5 == 0).map(|(_, s)| s.as_str()).collect::<String>());

		ans
	}
}

#[derive(Default, Debug)]
pub struct MiniCrosswordEnvExt {
	data: serde_json::Value,
	board_gt: Vec<String>,
	board: Vec<String>,
	ans: Vec<String>,
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
		"mini_crossword" => {
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
