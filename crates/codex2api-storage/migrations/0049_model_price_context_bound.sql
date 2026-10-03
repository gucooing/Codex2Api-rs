-- Some official prices apply only within a bounded input context (GPT-5.5 Fast).
-- Existing rules and historical snapshots remain unbounded unless explicitly set.
ALTER TABLE model_prices ADD COLUMN max_input_tokens INTEGER
    CHECK(max_input_tokens IS NULL OR max_input_tokens >= min_input_tokens);
