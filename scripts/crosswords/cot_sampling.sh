cargo run -- \
    --backend gpt-3.5-turbo \
    --task crosswords \
    --task_file_path mini0505_0_100_5.json \
    --task_start_index 0 \
    --task_end_index 20 \
    --naive_run \
    --prompt_sample cot \
    --n_generate_sample 10 