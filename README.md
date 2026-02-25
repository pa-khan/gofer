# gofer

**Intelligent MCP Server for Rust Codebases**

[![Rust](https://img.shields.io/badge/rust-1.76+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-MVP--testing-yellow.svg)](https://github.com/pa-khan/gofer)
[![GitHub](https://img.shields.io/badge/GitHub-pa--khan%2Fgofer-blue?logo=github)](https://github.com/pa-khan/gofer)

**[Русская версия](README.ru.md)** | **English**

---

## 📖 Table of Contents

- [About](#-about)
- [Why gofer?](#-why-gofer)
- [Tech Stack](#-tech-stack)
- [Current Stage](#-current-stage)
- [Installation](#-installation)
- [Features](#-features)
- [Usage Examples](#-usage-examples)
- [Roadmap](#-roadmap)
- [Documentation](#-documentation)
- [Contributing](#-contributing)

---

## 🚀 About

**gofer** is a high-performance MCP (Model Context Protocol) server that transforms how AI assistants interact with codebases. The system provides intelligent indexing, semantic search, and token-efficient code reading, reducing token consumption by **50-70%** in typical scenarios.

The name comes from "go for this, go for that" - a helper who does all the small tasks.

### Key Features

- 🔍 **Semantic search** with vector embeddings and re-ranking
- 📊 **AST parsing** via tree-sitter (Rust, TypeScript, Python, Go, Vue)
- 💾 **Hybrid storage**: SQLite (metadata) + LanceDB (vectors)
- ⚡ **Incremental indexing** with file watcher (50-100× faster)
- 🎯 **Token-efficient tools**: skeleton, context_bundle, read_function_context
- 🔄 **Batch operations**: multiple requests in one call
- 🧠 **LRU cache** on server side to reduce repeated queries
- 🛡️ **Circuit breakers** and graceful error recovery
- 📈 **Prometheus metrics** (port 9091)

---

## 💡 Why gofer?

### Problems with Existing Solutions

1. **Excessive token consumption**: reading entire files instead of necessary fragments
2. **Slow indexing**: full-scan on every change
3. **Lack of context**: no understanding of relationships between symbols and files
4. **Low search accuracy**: keyword-based search without semantics

### gofer Solution

- **50-70% token savings** via `skeleton`, `read_function_context`, `read_types_only`
- **Incremental indexing**: only changed files
- **Semantic search**: vector embeddings + BGE-reranker + hybrid mode
- **Dependency graph**: tracking relationships between symbols, files, and imports
- **Caching**: LRU cache with TTL and auto-invalidation
- **Batch API**: 3-5× latency reduction

---

## 🛠 Tech Stack

### Language and Runtime

- **Rust 2021 edition** (1.76+)
- **Tokio** — async runtime with multi-threading
- **jemalloc** — optimized memory allocator

### Parsing and Indexing

- **tree-sitter** (v0.24) — incremental AST parser
  - Support: Rust, TypeScript, Python, Go, HTML/Vue
- **SQLite** (sqlx v0.8) — relational DB for metadata
  - Symbols, files, references, dependencies, diagnostics
- **LanceDB** (v0.23) — vector DB for embeddings

### Embeddings and Semantic Search

- **fastembed** (v5) — fast embeddings
  - Model: `BGESmallENV15` (384-dimensional vectors)
- **ONNX Runtime** (ort v2.0-rc.11) — re-ranking model
  - Cross-encoder for accurate result ranking
- **ndarray** — matrix operations for scoring

### Git and Filesystem

- **git2** (v0.20) — libgit2 bindings for history analysis
- **notify** (v7) + **debouncer** — file watcher with debouncing
- **ignore** (v0.4) — `.gitignore` rules parsing

### IPC and Protocol

- **Unix Domain Sockets** — inter-process communication
- **JSON-RPC 2.0** — MCP protocol
- **serde_json** — data serialization

### Optimizations

- **rkyv** (v0.7) — zero-copy serialization for cache
- **blake3** — fast hashing for content-addressable storage
- **rayon** — parallel file processing

---

## 📍 Current Stage

### Status: **MVP — Proof of Concept**

The project is in active development and testing architectural decisions. Basic features are implemented, metrics are being collected, and performance is being optimized.

#### What Works Now ✅

- ✅ Daemon architecture with Unix socket IPC
- ✅ MCP protocol bridge (stdio ↔ daemon)
- ✅ Tree-sitter parsing for Rust, TS, Python, Go, Vue
- ✅ SQLite + LanceDB hybrid storage
- ✅ Semantic search with reranking
- ✅ Incremental indexing with file watcher
- ✅ Token-efficient tools (skeleton, context bundle)
- ✅ Batch operations API
- ✅ LRU cache with TTL
- ✅ Git integration (blame, history, diff)
- ✅ Prometheus metrics

#### In Development 🚧

- 🚧 Rust-analyzer LSP integration
- 🚧 Content-addressable storage (hash buffers)
- 🚧 Atomic transactions for file operations
- 🚧 Execution sandbox (safe code execution)
- 🚧 Smart commit generation

#### Known Limitations ⚠️

- Linux-only support (requires Unix socket)
- No GPU acceleration for embeddings (CPU only)
- No web UI for monitoring (Prometheus metrics only)
- Limited monorepo support (one project = one directory)

---

## 📦 Installation

### Requirements

#### System Requirements

- **OS**: Linux (x86_64)
- **CPU**: 4+ cores (8+ recommended)
- **RAM**: 4 GB minimum, 8+ GB recommended
- **Disk**: 2 GB for average project index storage

#### Software Dependencies

- **Rust**: 1.76 or newer
- **Git**: 2.30+ (for git integration)
- **SQLite**: 3.35+ (embedded in sqlx)

### Building from Source

```bash
# Clone repository
git clone https://github.com/pa-khan/gofer.git
cd gofer

# Fast build for development
cargo build --profile release-dev

# Final release build (slower, but maximum optimization)
cargo build --release

# Install binary
cargo install --path .
```

### Quick Start

```bash
# 1. Initialize daemon + register project
cd /path/to/your/project
gofer hi

# 2. Check status
gofer status

# 3. Run MCP server (for use with Qoder or other MCP clients)
gofer mcp

# 4. View logs
gofer logs -n 100 --follow

# 5. Stop daemon
gofer stop
```

### Alternative Commands

```bash
# Start daemon only (without project registration)
gofer daemon

# Register project without activation
gofer init

# Activate project with indexing
gofer start --watch

# Force reindex
gofer reindex --force

# Search codebase
gofer search "authentication logic" --limit 10

# View metrics
curl http://localhost:9091/metrics
```

### Configuration

```bash
# Create config file .gofer/config.toml
gofer config init

# View current configuration
gofer config

# Config file path
gofer config path
```

Example configuration:

```toml
[server]
port = 10987

[indexer]
ignore = ["*.test.ts", "mock/"]
parallel_workers = 4

[embedding]
batch_size = 32
model = "BGESmallENV15"
pool_size = 4

[reranker]
enabled = true
model_dir = ".gofer/data/models/reranker"

[summarizer]
enable_llm = true
model_id = "qwen2.5-coder:1.5b"
max_tokens = 150
temperature = 0.3
```

---

## 🎯 Features

Complete list of **100+ MCP tools** available via JSON-RPC protocol:

| Category | Tool | Description |
|----------|------|-------------|
| **Index Health** | `get_index_status` | Index status with completeness metrics |
| | `validate_index` | Index integrity check |
| | `force_reindex` | Force reindex (file/dir/project) |
| | `health_check` | Health check for all components |
| **Semantic Search** | `search` | Hybrid search with vectors + keywords + reranking |
| | `search_by_purpose` | Search files by purpose (architectural queries) |
| | `search_symbols` | Search symbols by name (substring matching) |
| | `search_files` | Regex search in file contents |
| | `cross_stack_search` | Search with cross-stack correlation (backend ↔ frontend) |
| **Token-Efficient Reading** | `skeleton` | Signatures only without function bodies (3-5× savings) |
| | `read_function_context` | One function + its dependencies (90-95% savings) |
| | `read_types_only` | Type definitions only (90-95% savings) |
| | `read_file` | Read file with optional line range |
| | `context_bundle` | File + dependencies with optional skeletonization |
| **Symbols & References** | `get_symbols` | List symbols (functions, structs, classes) |
| | `get_references` | All references to a symbol (where it's used) |
| | `get_callers` | Who calls this symbol (incoming refs) |
| | `get_callees` | What this symbol calls (outgoing refs) |
| | `symbol_exists` | Lightweight symbol existence check |
| | `is_exported` | Check if symbol is public |
| | `has_documentation` | Check for doc comments |
| **Dependencies & Graph** | `get_dependencies` | Dependencies from Cargo.toml/package.json |
| | `dependency_impact` | All files using a dependency |
| | `get_api_routes` | List API endpoints (backend + frontend) |
| | `domain_stats` | Code distribution statistics by domain |
| **Git Integration** | `git_blame` | Commit info for a line |
| | `git_history` | Commit history for a file |
| | `git_diff` | Diff for staged/unstaged changes |
| | `suggest_commit` | Generate commit message based on diff |
| **Diagnostics** | `get_errors` | Compiler errors (cargo check, tsc) |
| | `run_diagnostics` | Run cargo check/tsc to update diagnostics |
| | `run_check` | Run checks without modifying files |
| | `get_config_keys` | Config keys from .env.example |
| | `has_tests_for` | Check for tests for a file |
| **File Operations** | `list_directory` | List files and directories (recursive) |
| | `project_tree` | Project tree with .gitignore filtering |
| | `find_files` | Find files by glob pattern |
| | `grep` | Regex search in contents with line numbers |
| | `get_file_metadata` | File metadata (size, mtime, lines) |
| | `file_exists` | Lightweight file existence check |
| | `write_file` | Create/overwrite file |
| | `patch_file` | Precise substring replacement (search & replace) |
| | `append_to_file` | Append to end of file |
| | `move_file` | Move/rename file |
| | `create_directory` | Create directory (with mkdir -p) |
| **Trash Management** | `delete_safe` | Safe delete to trash (with metadata) |
| | `list_trash` | Trash contents |
| | `restore` | Restore from trash |
| | `purge_trash` | Permanently delete from trash |
| **Transactions** | `begin_transaction` | Start atomic transaction |
| | `add_operation` | Add operation to transaction |
| | `commit_transaction` | Apply all operations atomically |
| | `rollback_transaction` | Cancel transaction without applying |
| | `list_transactions` | List active transactions |
| **Formatting & Linting** | `format_file` | Auto-format (rustfmt, prettier, black) |
| | `lint_file` | Lint (clippy, eslint, ruff) |
| | `apply_lint_fix` | Apply auto-fix from linter |
| | `verify_patch` | Verify patch with compiler (no changes) |
| **Content-Addressable Storage** | `extract_to_hash` | Extract code block to hash (token savings) |
| | `insert_hash` | Insert code from hash by line number |
| | `replace_with_hash` | Replace code block from hash |
| | `content_to_hash` | Convert content to hash |
| | `list_buffers` | List active hashes in memory |
| | `clear_buffer` | Clear hash buffer |
| **Sandbox Execution** | `execute_code` | Execute code in isolated environment |
| | `execute_function` | Execute specific function with arguments |
| **Batch Operations** | `batch_operations` | Multiple read/search operations in one request |
| **Optimization** | `smart_file_selection` | AI hints for selecting relevant files |
| | `get_cache_stats` | Cache statistics (hit rate, sizes) |
| **Project** | `add_rule` | Add rule/best practice to context |
| | `mark_golden_sample` | Mark file as reference example |
| | `get_summary` | AI summary of file purpose |
| | `get_vue_tree` | Vue component DOM tree |
| **Language Services** | `lang_tools_list` | List language-specific tools |
| | `lang_tools_call` | Call language-specific tool (Vue, Rust LSP, etc.) |

### Specialized Tools (via `lang_tools_call`)

#### Rust-analyzer tools (in development)

- `rust_goto_definition` — go to definition
- `rust_find_references` — find all references
- `rust_hover` — type and documentation for symbol
- `rust_diagnostics` — diagnostics from rust-analyzer
- `rust_completions` — code completion
- `rust_inlay_hints` — inline type hints
- `rust_code_actions` — quick fixes and refactorings
- `rust_document_symbols` — file outline
- `rust_workspace_symbols` — search symbols in workspace
- `rust_goto_implementation` — go to trait implementation
- `rust_rename` — semantic refactoring (rename)
- `rust_expand_macro` — macro expansion

---

## 📚 Usage Examples

### Example 1: Semantic Search

```bash
# CLI
gofer search "error handling middleware" --limit 5
```

```jsonc
// MCP tool call
{
  "name": "search",
  "arguments": {
    "query": "error handling middleware",
    "limit": 5,
    "include_scores": true,
    "min_score": 0.5
  }
}

// Response
{
  "results": [
    {
      "file_path": "src/middleware/error_handler.rs",
      "line_start": 10,
      "content": "pub async fn error_middleware(req: Request, next: Next) -> Response { ... }",
      "score": 0.92
    }
  ]
}
```

### Example 2: Token-Efficient Reading

```jsonc
// Full file (3000 tokens)
{ "name": "read_file", "arguments": { "file": "src/main.rs" } }

// Signatures only (600 tokens, 80% savings)
{ "name": "skeleton", "arguments": { "file": "src/main.rs" } }

// One function + deps (200 tokens, 93% savings)
{
  "name": "read_function_context",
  "arguments": {
    "file": "src/main.rs",
    "function": "main",
    "include_imports": true
  }
}
```

### Example 3: Batch Operations

```jsonc
// Instead of 3 separate requests — one batch call
{
  "name": "batch_operations",
  "arguments": {
    "operations": [
      { "type": "read_file", "params": { "file": "src/main.rs" } },
      { "type": "get_symbols", "params": { "file": "src/main.rs" } },
      { "type": "search", "params": { "query": "async fn", "limit": 5 } }
    ],
    "parallel": true
  }
}
```

### Example 4: Atomic Transactions

```jsonc
// 1. Start transaction
{ "name": "begin_transaction", "arguments": { "transaction_id": "refactor-001" } }

// 2. Add operations
{
  "name": "add_operation",
  "arguments": {
    "transaction_id": "refactor-001",
    "operation": {
      "type": "patch_file",
      "params": {
        "path": "src/main.rs",
        "search_string": "old_function_name",
        "replace_string": "new_function_name"
      }
    }
  }
}

// 3. Commit (atomic application of all operations)
{ "name": "commit_transaction", "arguments": { "transaction_id": "refactor-001" } }
```

---

## 🗺 Roadmap

### Phase 0: Active Testing (current stage) ✅

**Goal**: MVP stabilization, metrics collection, critical bug fixes

- ✅ Basic functionality (search, read, symbols)
- ✅ Daemon architecture with IPC
- ✅ Incremental indexing
- 🚧 Rust-analyzer LSP integration
- 🚧 Performance profiling and optimization
- 🚧 Comprehensive tests (unit + integration)

**Success Metrics**:
- 95%+ index completeness
- < 5s cold start for average project (10k files)
- 40%+ cache hit rate
- 50-70% token savings in real scenarios

---

### Phase 1: Language Isolation with Wasmtime (3-4 weeks)

**Goal**: Safe user code execution

**Tasks**:
- Compile Rust/Python/JS code to WASM
- Wasmtime runtime with resource limits (CPU, memory, time)
- Sandboxed file system via WASI
- API for `execute_code` and `execute_function`

**Deliverables**:
- ✨ Safe code execution in isolated environment
- ✨ Resource limits: 1 CPU core, 512 MB RAM, 5s timeout
- ✨ Blocked syscalls: network, filesystem (except WASI)

**Risks**:
- Complexity of WASM compilation for some languages
- Startup overhead (mitigated: warm pool of instances)

---

### Phase 2: Optimizations (4-6 weeks)

**Goal**: Production-ready performance and scalability

**Backend Optimizations**:
- Async embedder with batching and priority queue
- Incremental vector indexing (append-only LanceDB)
- Query planner with cost estimation
- Smart prefetching based on access patterns

**Index Optimizations**:
- Compressed embeddings (int8 quantization)
- Bloom filters for symbol_exists
- Inverted index for full-text search
- Partitioning by language/directory

**Network Optimizations**:
- HTTP/2 for MCP protocol (instead of Unix socket)
- Streaming responses for large results
- Request deduplication (within 100ms)

**Success Metrics**:
- < 1s latency for 95% of requests
- 10k+ RPS on single daemon instance
- < 100 MB RAM overhead per project
- 60%+ cache hit rate

---

### Phase 3: Production (6-8 weeks)

**Goal**: Enterprise-ready deployment and monitoring

**Features**:
- Multi-project support with isolation (namespaces)
- Distributed indexing (master/worker architecture)
- High availability (leader election, failover)
- Authentication and authorization (JWT/OAuth)
- Rate limiting and quotas per user
- Audit log for all operations

**Observability**:
- Structured JSON logging
- Prometheus metrics (latency, throughput, errors)
- Distributed tracing (OpenTelemetry)
- Health checks and readiness probes
- Grafana dashboards

**Operations**:
- Docker image + Kubernetes manifests
- Helm chart for deployment
- CI/CD pipeline (GitHub Actions)
- Database migrations strategy
- Backup and restore procedures

**Documentation**:
- Production deployment guide
- Security best practices
- Troubleshooting runbook
- API reference (OpenAPI spec)

---

### Phase 4+: Advanced Features (Future)

**Potential Directions**:
- Multi-language support expansion (Java, C++, PHP)
- Machine learning models for code completion
- Collaborative features (shared annotations, discussions)
- IDE plugins (VSCode, IntelliJ, Neovim)
- Cloud-hosted SaaS version

---

## 📖 Documentation

### Documentation Structure

```
docs/
├── desc/
│   ├── OVERVIEW.md           # Comprehensive technical overview
│   ├── INDEX.md              # Feature navigation
│   ├── phase-0/              # Phase 0 feature specs (16 files)
│   ├── phase-1/              # Phase 1 feature specs (12 files)
│   ├── phase-2/              # Phase 2 feature specs (11 files)
│   └── phase-3/              # Phase 3 feature specs (9 files)
├── features/                 # Original feature designs (21 files)
├── next_stage/               # Future roadmap and extensions
│   ├── IMPLEMENTATION_PLAN.md
│   ├── ROADMAP.md
│   └── OPTIMIZATION_OPPORTUNITIES.md
└── QUICK_START_GUIDE.md      # Quick start for users
```

### Useful Links

- **[Comprehensive Overview](docs/desc/OVERVIEW.md)** — complete architecture and features overview
- **[Feature Index](docs/desc/INDEX.md)** — navigation through all 48+ features
- **[Implementation Plan](docs/next_stage/IMPLEMENTATION_PLAN.md)** — detailed plan for phases 4-5
- **[Quick Start](docs/QUICK_START_GUIDE.md)** — quick start for beginners

---

## 🤝 Contributing

We welcome contributions from the community!

### How to Help

1. **MVP Testing**: use gofer on your projects and report bugs
2. **Documentation**: improve guides, add examples
3. **Performance benchmarks**: compare with alternatives
4. **Feature requests**: suggest new features via GitHub Issues

### Development Setup

```bash
# Clone with submodules
git clone --recursive https://github.com/pa-khan/gofer.git
cd gofer

# Install pre-commit hooks
cargo install cargo-watch
cargo install cargo-nextest

# Run tests
cargo nextest run

# Run in dev mode with hot reload
cargo watch -x 'run -- daemon'
```

### Guidelines

- Follow Rust style guide (rustfmt)
- Add unit tests for new features
- Document public APIs in docstrings
- Use conventional commits

---

## 📝 License

MIT License. See [LICENSE](LICENSE) for details.

---

## 🙏 Acknowledgments

- **tree-sitter** — for the powerful incremental parser
- **LanceDB** — for the fast vector database
- **Tokio** — for the production-ready async runtime
- **Rust community** — for the ecosystem of quality libraries

---

## 📞 Contacts

- **Issues**: [GitHub Issues](https://github.com/pa-khan/gofer/issues)
- **Discussions**: [GitHub Discussions](https://github.com/pa-khan/gofer/discussions)

---

**Made with ❤️ in Rust using [Qoder](https://qoder.com) and [Gemini](https://deepmind.google/technologies/gemini/)**
