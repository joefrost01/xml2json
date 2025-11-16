# Architecture & Design Decisions

This document explains the key architectural decisions and design philosophy behind xml-to-ndjson.

## Design Philosophy

The implementation follows these core principles:

1. **Simplicity First**: The number one feature is simple
2. **Pragmatic Over Perfect**: Solutions optimized for the problem space
3. **Composable Components**: Modular design that intersects with the domain
4. **Idiomatic When Readable**: Rust best practices where they aid comprehension
5. **Documentation Drives Adoption**: Clear docs for users and developers

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI Layer                            │
│                    (clap argument parsing)                   │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    Storage Abstraction                       │
│         (Trait: LocalStorage, GcsStorage)                    │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                   Converter Orchestration                    │
│         (File discovery, Rayon parallelization)              │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                  Per-File Processing Pipeline                │
│   Stream Read → Parse XML → Convert JSON → Write NDJSON     │
└─────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. NDJSON as Primary Format

**Decision**: Start with NDJSON, not Parquet

**Rationale**:
- BigQuery has native NDJSON support with auto-schema detection
- Text format is debuggable (can `cat` and inspect)
- No schema management complexity upfront
- Streaming-friendly (write line-by-line)
- Easy validation and troubleshooting

**Trade-offs**:
- Larger file size vs Parquet
- Slower BigQuery load vs Parquet
- No built-in compression

**Future**: Parquet support can be added as an option when needed

### 2. Storage Abstraction via Traits

**Decision**: Use trait-based abstraction for storage

**Rationale**:
- Single interface for local and cloud storage
- Easy to test (can mock storage)
- Extensible to other cloud providers
- Composable with the converter logic

**Implementation**:
```rust
trait Storage: Send + Sync {
    fn list_files(&self, prefix: &str) -> Result<Vec<String>>;
    fn read(&self, path: &str) -> Result<Box<dyn Read + Send>>;
    fn write(&self, path: &str) -> Result<Box<dyn Write + Send>>;
}
```

### 3. File-Level Parallelism with Rayon

**Decision**: Parallelize at the file level, not message level

**Rationale**:
- Files are independent units of work
- Perfect for Rayon's work-stealing algorithm
- Avoids thread coordination overhead
- Maximizes CPU utilization
- Each file streams independently (good memory locality)

**Why not message-level parallelism?**
- Would require thread synchronization for writes
- More complex with minimal performance benefit
- Breaks streaming processing model

### 4. Streaming XML Processing

**Decision**: Stream files using `quick-xml` event-based parsing

**Rationale**:
- Constant memory usage (critical for 300M messages)
- Process files larger than available RAM
- `quick-xml` is the fastest Rust XML parser
- No DOM tree construction overhead

**Memory Profile**:
- File size: Doesn't matter
- Total messages: Doesn't matter
- Memory usage: Constant (~few MB per thread)

### 5. Structure Preservation

**Decision**: Use `serde` to automatically preserve XML structure

**Rationale**:
- Handles nested elements correctly
- Preserves attributes
- No manual mapping required
- Leverages Rust's excellent serialization ecosystem

**Alternative considered**: Manual JSON construction
- Rejected: More code, more bugs, not simpler

### 6. Error Handling Strategy

**Decision**: Use `thiserror` for domain errors, `anyhow` for generic errors

**Rationale**:
- `thiserror` provides structured error types for the library
- `anyhow` simplifies error handling in the CLI
- Clear error messages for debugging
- Doesn't abort on single file failure (collects errors)

**User Experience**:
- Processes all files even if some fail
- Reports all errors at the end
- Non-zero exit code if any failures

### 7. Zero Configuration

**Decision**: Minimal required arguments, smart defaults

**Rationale**:
- Source and destination are all you need
- Auto-detects storage type from path (gs:// vs local)
- Default message element name works for common cases
- Progress bar enabled by default (but can disable)

**CLI Design**:
```bash
# Minimal invocation
xml-to-ndjson --source /xml --destination /json

# Everything else is optional
--message-element <name>
--no-progress
```

## Module Responsibilities

### `main.rs` - CLI Entry Point
- Parse arguments
- Validate configuration
- Initialize storage
- Run converter
- Handle exit codes

**Design**: Keep it simple, just wiring

### `cli.rs` - Argument Parsing
- Define CLI structure with `clap`
- Validation logic
- Help text

**Design**: Declarative with derive macros

### `storage.rs` - Storage Abstraction
- `Storage` trait definition
- `LocalStorage` implementation
- `GcsStorage` implementation
- Storage factory function

**Design**: Trait-based polymorphism, single responsibility

### `converter.rs` - Core Logic
- File discovery and listing
- Parallel processing orchestration
- Per-file conversion pipeline
- Statistics collection

**Design**: Functional composition, clear data flow

### `error.rs` - Error Types
- Domain-specific error types
- Error conversions
- Type aliases for `Result`

**Design**: Structured errors for better debugging

## Performance Characteristics

### Time Complexity
- File listing: O(n) where n = number of files
- Conversion: O(n) where n = total size of all files
- Parallelization: Linear speedup with cores (embarrassingly parallel)

### Space Complexity
- Per-thread: O(1) - streaming processing
- Total: O(c) where c = number of cores (thread-local buffers)

### Expected Performance
For 300M messages across 10K files on an 8-core machine:
- Conversion time: Minutes (not hours)
- Memory usage: < 1GB
- Bottleneck: Usually GCS upload bandwidth, not CPU

## Testing Strategy

### Unit Tests
- Path manipulation logic
- XML to JSON conversion
- Error handling

### Integration Tests
- Full pipeline with sample data
- Storage abstraction implementations

### Manual Testing
```bash
just run-sample  # Quick smoke test
```

## Future Extensibility

### Adding Parquet Support
1. Add `parquet` crate dependency
2. Create `ParquetWriter` in converter
3. Add `--format` CLI argument
4. Implement schema inference from first N messages

### Adding Other Storage Backends
1. Implement `Storage` trait
2. Update `create_storage()` factory
3. Add URL pattern matching

### Adding Compression
1. Wrap writers with compression streams
2. Add `--compression` CLI argument
3. Update file extensions accordingly

## Trade-offs Made

| Decision | Pro | Con | Mitigation |
|----------|-----|-----|------------|
| NDJSON not Parquet | Simple, debuggable | Larger files | Can add Parquet later |
| File-level parallelism | Simple, efficient | Can't parallelize huge single files | Assumes many files |
| Streaming processing | Low memory | Can't do multi-pass | Not needed for conversion |
| Trait objects | Flexible | Small runtime cost | Negligible for I/O bound work |

## Code Organization Principles

1. **Each module has a single responsibility**
2. **Public API is minimal** (most things are `pub(crate)`)
3. **Types are validated at construction** (makes illegal states unrepresentable)
4. **Errors are descriptive** (include context)
5. **Tests are co-located** (in same file as code)

## Alignment with Project Goals

✅ **Simple**: Zero-config for basic use, minimal dependencies  
✅ **Fast**: Parallel + streaming = optimal performance  
✅ **Cross-platform**: Pure Rust, works everywhere  
✅ **Testable**: Easy to test locally before production  
✅ **Maintainable**: Clean architecture, well documented  
✅ **Pragmatic**: Optimized for the actual problem, not textbook perfection  

## Lessons for Future Development

1. **Start simple, add complexity only when needed**
   - We chose NDJSON over Parquet initially
   
2. **Measure before optimizing**
   - File-level parallelism is likely sufficient
   
3. **Design for observability**
   - Progress bars and statistics help debugging
   
4. **Make errors actionable**
   - Error messages suggest solutions
   
5. **Document the "why" not just the "what"**
   - This document exists for a reason
