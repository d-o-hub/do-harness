-- `list_heuristics` is the whole read path for this table and filters by skill
-- (`WHERE skill_name = ?1 ORDER BY id`), which had no index. The ordering needs
-- no second indexed column: every index entry of a rowid table ends with the
-- rowid, and `heuristics.id` is that rowid, so entries are already in `id`
-- order within one `skill_name`
-- (https://www.sqlite.org/fileformat2.html, section 2.5.1).
CREATE INDEX IF NOT EXISTS idx_heuristics_skill_name ON heuristics(skill_name);
