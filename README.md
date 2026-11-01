# kotlin-syntax

A lossless Kotlin lexer and parser in Rust, ported from the Kotlin compiler's own (`Kotlin.flex`, `KotlinParsing`,
`KotlinExpressionParsing`, IntelliJ's `PsiBuilder`). Its tree prints exactly what Kotlin's
`DebugUtil.psiToString(file, true, false)` prints. See AGENTS.md for the design and the oracles.

```rust
use kotlin_syntax::{FileKind, ParseOptions, Parser};

let mut parser = Parser::new(); // one per thread; reuses its buffers from file to file
let tree = parser.parse(b"fun f() = 1", FileKind::Kt); // Tree: Send + Sync, ids in preorder
for (token, range) in tree.leaves() { /* exact highlighting: soft keywords and ?. ?: !! as the parser sees them */ }
let decls = parser.parse_with_options(b"fun f() { g() }", FileKind::Kt, ParseOptions { expand_bodies: false });
```

AGENTS.md, "The consumer contract", says what a consumer may rely on.

Licensed under the Apache License, Version 2.0; see NOTICE for the code it ports.
