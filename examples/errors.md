# Errors are content, not crashes

## Invalid flowchart

```mermaid
flowchart LR
  A[unfinished
```

## Unsupported diagram type

```mermaid
pie
    "Not supported in tpreview" : 100
```

## A statement that must not be silently omitted

```mermaid
sequenceDiagram
    participant A
    unsupported semantic statement
    A->>B: this must not appear as a complete successful render
```
