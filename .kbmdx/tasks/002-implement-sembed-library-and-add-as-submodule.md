---
id: 2
title: Implement sembed library and add as submodule
status: done
priority: high
created: 2026-03-10T10:00:32.353288Z
updated: 2026-03-11T18:41:59.921999Z
started: 2026-03-10T10:15:45.151996Z
completed: 2026-03-10T10:15:45.151996Z
tags:
- layer-4
class: standard
---

# Implement sembed library

Create github.com/aeronmiles/sembed as a standalone Go library, then add it as a git submodule to kanban-mdx.

## Reference

See task #1 for full design spec and library landscape assessment.

## Deliverables

### 1. Create sembed repository on GitHub (github.com/aeronmiles/sembed)

- [ ] Initialize Go module (go 1.25, MIT license)
- [ ] Zero third-party dependencies — stdlib only

### 2. Core types (sembed.go)

- [ ] Embedder interface: Embed(ctx, []string) ([]Vector, error)
- [ ] Vector type ([]float32) with CosineSimilarity, Dot, Normalize methods
- [ ] Document struct: ID, Content, ContentHash, Vector, Metadata
- [ ] Result struct: Document + Score
- [ ] Option type for provider configuration (WithDimensions, WithInputType)

### 3. Vector math (vector.go)

- [ ] CosineSimilarity(a, b Vector) float32
- [ ] Dot(a, b Vector) float32
- [ ] Normalize(v Vector) Vector
- [ ] Unit tests with known vectors

### 4. Flat index (index.go, index_json.go)

- [ ] NewIndex() — concurrent-safe via sync.RWMutex
- [ ] Add(docs ...Document) — upsert by ID
- [ ] Remove(ids ...string)
- [ ] Get(id string) (Document, bool)
- [ ] Search(query Vector, k int) []Result — brute-force cosine ranked
- [ ] Stale(current map[string]string) []string — content hash comparison
- [ ] All() []Document, Len() int
- [ ] Save(w io.Writer) / Load(r io.Reader) — JSON serialization
- [ ] Unit tests: add/remove/upsert, search ranking, stale detection, save/load round-trip

### 5. OpenAI-compatible provider (openai.go)

- [ ] OpenAIConfig struct: BaseURL, APIKey, Model, Dimensions, InputType
- [ ] NewOpenAICompatible(cfg) Embedder — HTTP POST to /v1/embeddings
- [ ] VoyageAI(apiKey, model, ...Option) convenience constructor (base URL: https://api.voyageai.com/v1)
- [ ] OpenAI(apiKey, model, ...Option) convenience constructor (base URL: https://api.openai.com/v1)
- [ ] Error handling: HTTP status codes, rate limits, context cancellation
- [ ] Unit tests with httptest server mocking the /v1/embeddings response

### 6. Ollama provider (ollama.go)

- [ ] OllamaConfig struct: BaseURL (default localhost:11434), Model
- [ ] NewOllama(cfg) Embedder — HTTP POST to /api/embed
- [ ] Unit tests with httptest mock

### 7. Content hashing (hash.go)

- [ ] ContentHash(content string) string — SHA-256 hex
- [ ] Unit test: deterministic, different content → different hash

### 8. Add as submodule to kanban-mdx

- [ ] git submodule add github.com/aeronmiles/sembed
- [ ] Verify go module resolution

## Acceptance criteria

- go test ./... passes with 100% of tests green
- go vet ./... clean
- Zero third-party dependencies (only stdlib)
- All public types and functions have doc comments
- README.md with usage examples for each provider
