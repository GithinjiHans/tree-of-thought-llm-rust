use std::{collections::BTreeMap, path::Path};

pub const DATA_PATH: &str = "./data";

#[derive(Debug)]
pub(crate) enum Task {
	Game24 {
		data: Vec<Box<str>>,
		stops: [char; 4],
		steps: isize,
		value_cache: BTreeMap<String, String>,
	},
	Text {
		data: Vec<Box<str>>,
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
				puzzles.push(Box::<str>::from(record.get(1).unwrap()));
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
			let lines = std::fs::read_to_string(path)?.lines().map(|s| Box::<str>::from(s)).collect::<Vec<_>>();

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
