# Data Model: The Minimal Python Demo

| Entity | Fields | Rule |
|---|---|---|
| **Document** | `id` (`doc-NN`), `text` (`"<title>\n\n<body>"`) | ten in the file; passage-sized; one `contents` text field, also the dense field |
| **Argument** | `query` | exactly one; none → `usage: demo.py "your question"`, exit 2 |
| **Run** | `fused` (depth 0, k 5), `reranked` (depth 10, k 5) — the engine's responses | the index lives in a temporary directory removed after the run |
| **Hit line** | rank, id, `score` (the engine's, 4 decimals), `rerank` (re-ranked list only), the text's first line | from the engine's `Hit` |
| **Oracle** (test) | the same documents built directly through the package | ids in order and score bits equal at both depths; two runs identical |
