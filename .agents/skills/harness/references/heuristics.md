# Heuristics
- **task done refuses until verify --record --task <id> records the named sensor beat**: workflow gate on task completion (from trace 1)
- **persistence commands failing with FutureDatabase means the release binary predates the state schema: rebuild (cargo build --release -p do-harness); doctor now flags binary/database migration skew before any command dies**: stale binary vs migrated agent_state.db (from trace 2)
