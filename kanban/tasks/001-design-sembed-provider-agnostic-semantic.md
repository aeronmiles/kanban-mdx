---
id: 1
title: 'Design sembed: provider-agnostic semantic embeddings library for Go'
status: done
priority: high
created: 2026-03-10T09:59:19.040059Z
updated: 2026-03-12T19:19:03.515535Z
started: 2026-03-10T14:45:35.146230Z
completed: 2026-03-10T14:45:35.146230Z
tags:
- layer-4
- idea
class: standard
branch: main
---

# sembed — Provider-Agnostic Semantic Embeddings Library for Go

## Context

kanban-md needs opt-in semantic search: conceptual retrieval across tasks when substring matching falls short (e.g. "performance tasks" finding a task titled "Optimize board rendering"). This must not compromise kanban-md's core identity — file-based, single-binary, zero-infra, works-offline. The solution is a standalone Go library (separate repo, added as git submodule) that kanban-md imports optionally.

## Go Library Landscape Assessment

### Embedding Clients

| Library | Providers | Interface | License | Verdict |
|---|---|---|---|---|
| kshard/embeddings | 3 (Bedrock, OpenAI, word2vec) | Embedding(ctx, string) — single-text only | MIT | Too limited. No Voyage AI, no batch. Good interface inspiration. |
| milosgajdos/go-embeddings | 6 (OpenAI, Cohere, Vertex, Voyage AI, Ollama, Bedrock) | Generic Embedder[T] | Apache-2.0 | Best provider coverage. Has Voyage AI. But purely an API client — no storage, no search. Complex generics. |

### Vector Storage/Search

| Library | Approach | Deps | License | Verdict |
|---|---|---|---|---|
| chromem-go | In-memory + gob persistence, brute-force cosine, zero deps | Zero | AGPL-3.0 | Excellent perf (1K docs/0.3ms). Full vector DB — overkill. AGPL restrictive. gob = Go-only format. |
| gaspiman/cosine_similarity | Pure cosine function | Zero | MIT | Reference only, not a library. |

### Conclusion

None suitable to fork or use verbatim. Build purpose-built library combining:
- Minimal interface ethos of kshard
- Provider breadth of milosgajdos (simplified — Voyage uses OpenAI-compatible HTTP, one client covers both)
- Brute-force cosine search from chromem-go (trivial ~30 lines)
- JSON persistence (human-readable, git-friendly)

## Design: github.com/aeronmiles/sembed

### File Layout

```
sembed/
├── sembed.go        # Core types: Vector, Document, Result, Embedder interface
├── vector.go        # CosineSimilarity, Dot, Normalize
├── index.go         # Flat in-memory index: Add, Remove, Search, Stale, Len
├── index_json.go    # Save/Load via JSON to io.Writer/io.Reader
├── openai.go        # OpenAI-compatible HTTP embedder (covers OpenAI + Voyage AI + Azure + vLLM + LiteLLM)
├── ollama.go        # Ollama embedder (different API format)
├── hash.go          # ContentHash (SHA-256) for staleness detection
├── go.mod
├── go.sum
├── LICENSE          # MIT
└── README.md
```

### Core Interface (sembed.go)

```go
package sembed

type Embedder interface {
    Embed(ctx context.Context, texts []string) ([]Vector, error)
}

type Vector []float32

type Document struct {
    ID          string            json:"id"
    Content     string            json:"content"
    ContentHash string            json:"content_hash"
    Vector      Vector            json:"vector"
    Metadata    map[string]string json:"metadata,omitempty"
}

type Result struct {
    Document Document json:"document"
    Score    float32  json:"score"
}
```

Design rationale:
- Embed(ctx, []string) — batch-first, all providers support batching
- Vector as []float32 — universal representation
- ContentHash on Document — enables staleness detection without Embedder
- Separate Embedder and Index — Index doesn't care how embeddings were generated

### Index (index.go)

```go
type Index struct { /* sync.RWMutex for concurrent safety */ }

func NewIndex() *Index
func (idx *Index) Add(docs ...Document) error    // upsert by ID
func (idx *Index) Remove(ids ...string)
func (idx *Index) Get(id string) (Document, bool)
func (idx *Index) All() []Document
func (idx *Index) Len() int
func (idx *Index) Search(query Vector, k int) []Result
func (idx *Index) Stale(current map[string]string) []string
func (idx *Index) Save(w io.Writer) error         // JSON
func (idx *Index) Load(r io.Reader) error          // JSON merge
```

No magic persistence. Caller controls when/where to save. For kanban-md: kanban/.embeddings.json.

### OpenAI-Compatible Provider (openai.go)

```go
type OpenAIConfig struct {
    BaseURL    string // e.g. "https://api.voyageai.com/v1"
    APIKey     string
    Model      string // e.g. "voyage-4-lite"
    Dimensions int    // optional (0 = default)
    InputType  string // optional: "query"/"document" (Voyage AI)
}

func NewOpenAICompatible(cfg OpenAIConfig) Embedder
func VoyageAI(apiKey, model string, opts ...Option) Embedder
func OpenAI(apiKey, model string, opts ...Option) Embedder
```

Key insight: Voyage AI /v1/embeddings uses same JSON schema as OpenAI. One HTTP client covers both (and Azure, vLLM, LiteLLM). Voyage's query-vs-document input_type is the only addition.

For Voyage's input_type distinction, create two embedders:
```go
docEmbed   := sembed.VoyageAI(key, "voyage-4-lite", sembed.WithInputType("document"))
queryEmbed := sembed.VoyageAI(key, "voyage-4-lite", sembed.WithInputType("query"))
```

### Ollama Provider (ollama.go)

```go
type OllamaConfig struct {
    BaseURL string // default: http://localhost:11434
    Model   string // e.g. "nomic-embed-text"
}
func NewOllama(cfg OllamaConfig) Embedder
```

Separate implementation because Ollama uses POST /api/embed with a different schema.

### kanban-md Integration Plan

Config (opt-in):
```yaml
semantic_search:
  enabled: true
  provider: voyage    # or openai, ollama, custom
  model: voyage-4-lite
  dimensions: 256
  # api_key from KANBAN_EMBED_API_KEY env var
```

- Embedding index at kanban/.embeddings.json
- CLI: list --semantic "query"
- TUI: /~query syntax triggers semantic search
- Falls back to substring search if API unavailable
- Content hashing detects stale embeddings without API call

### Voyage AI 4 Lite Specifics

- Price: $0.02/1M tokens, first 200M free
- Dimensions: 256/512/1024/2048 (256 recommended for kanban scale = ~32 bytes/task)
- Context: 32K tokens
- Quantization: float32, int8, uint8, binary, ubinary
- Shared embedding space with voyage-4, voyage-4-large, voyage-4-nano — can upgrade without re-indexing
- API: POST https://api.voyageai.com/v1/embeddings (OpenAI-compatible)

### Explicitly Out of Scope for v1

- ANN indexes (HNSW, IVF) — brute-force sufficient for <10K docs
- Quantized storage (int8/binary) — float32 @ 256 dims = 1KB/doc
- Auto-embedding on write — caller controls pipeline
- Text chunking/splitting — kanban tasks are short

### References

- Voyage AI docs: https://docs.voyageai.com/docs/embeddings
- Voyage 4 family: https://blog.voyageai.com/2026/01/15/voyage-4/
- chromem-go: https://github.com/philippgille/chromem-go
- kshard/embeddings: https://github.com/kshard/embeddings
- milosgajdos/go-embeddings: https://github.com/milosgajdos/go-embeddings
