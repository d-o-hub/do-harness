# Anti-AI-Slop Checklist

Post-rewrite and distillation audit checklist to ensure distilled heuristics, code examples, and skill artifacts remain idiomatic, concise, and free from AI boilerplate markers.

## Audit Checklist (Review Pass)

1. **Purpose-per-struct**: Every struct and type serves an immediate, distinct domain purpose. No empty wrapper structs, single-use tuple wrappers without domain meaning, or dummy structs created solely to satisfy boilerplate patterns.
2. **Real error handling**: Errors are preserved and contextually mapped rather than lazily discarded. No copy-paste `.map_err(|e| anyhow!(e.to_string()))` or swallowing error context. Preserve error chains (`thiserror` / `anyhow` context).
3. **Meaningful names**: Identifiers reflect concrete domain concepts. Eliminate generic AI placeholders like `data`, `item`, `process_info`, `handle_stuff`, `item_list`, or `result_wrapper`.
4. **No speculative abstraction**: Code directly addresses the specific problem without premature generalization. Avoid unnecessary generic parameters, single-impl traits, or layered factory abstractions created "just in case".
5. **Docs that say why (not what)**: Doc comments explain design rationale, non-obvious invariants, or trade-offs. Remove hollow `/// Does x` comments that merely echo function signatures.

---

## Seed Examples (<30 lines each)

### Example 1: Purpose-per-Struct & Speculative Abstraction

**Before (AI Slop):**
```rust
// Speculative trait + single-use wrapper struct
pub trait DataProcessor {
    fn process(&self, input: String) -> Result<ProcessOutput, ProcessError>;
}

pub struct ProcessOutput {
    pub value: String,
}

pub struct DefaultDataProcessor;

impl DataProcessor for DefaultDataProcessor {
    fn process(&self, input: String) -> Result<ProcessOutput, ProcessError> {
        Ok(ProcessOutput { value: input.trim().to_lowercase() })
    }
}
```

**After (Idiomatic):**
```rust
/// Normalizes user-provided tag strings for indexing.
pub fn normalize_tag(input: &str) -> String {
    input.trim().to_lowercase()
}
```

### Example 2: Error Handling & Hollow Docs

**Before (AI Slop):**
```rust
/// Helper to read file content
pub fn read_file_content(path: &str) -> Result<String, String> {
    // Copy-paste map_err discarding error context
    std::fs::read_to_string(path).map_err(|e| format!("Error occurred: {}", e))
}
```

**After (Idiomatic):**
```rust
/// Reads task state snapshot from disk, preserving file path context for diagnostics.
pub fn read_snapshot(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("failed to read task snapshot from {}", path.display()))
}
```

### Example 3: Meaningful Names & Unnecessary Wrapping

**Before (AI Slop):**
```rust
pub struct ItemListManager {
    pub items_data: Vec<String>,
}

impl ItemListManager {
    pub fn process_items_data(&mut self, item_info: String) {
        self.items_data.push(item_info);
    }
}
```

**After (Idiomatic):**
```rust
pub struct SkillCatalog {
    skills: Vec<SkillName>,
}

impl SkillCatalog {
    pub fn register(&mut self, skill: SkillName) {
        self.skills.push(skill);
    }
}
```
