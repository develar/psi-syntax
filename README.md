# psi-syntax

Lossless lexers and parsers in Rust, ported from the ones behind IntelliJ's PSI, whose trees print exactly what
`DebugUtil.psiToString(file, true, false)` prints. A Cargo workspace:

- `psi-builder`: the language-neutral half, IntelliJ's `PsiBuilder` (markers, production, edge binders), the preorder
  column tree and the parse driver that expands lazily parsed elements in place;
- `kotlin-syntax`: Kotlin, ported from the Kotlin compiler's own (`Kotlin.flex`, `KotlinParsing`,
  `KotlinExpressionParsing`, KDoc);
- `java-syntax`: Java, ported from IntelliJ's `java-syntax`: the Java and JavaDoc lexers (equal to IntelliJ's on every
  file tested), the parsers in progress;
- `ksdump`: dumps, benchmarks and lexes listed files like the PSI oracles.

See AGENTS.md for the porting rules and the oracles, docs/DESIGN.md for the architecture and its measurements.

```rust
use kotlin_syntax::{FileKind, ParseOptions, Parser};

let mut parser = Parser::new(); // one per thread; reuses its buffers from file to file
let tree = parser.parse(b"fun f() = 1", FileKind::Kt); // Tree: Send + Sync, ids in preorder
for (token, range) in tree.leaves() { /* exact highlighting: soft keywords and ?. ?: !! as the parser sees them */ }
let decls = parser.parse_with_options(b"fun f() { g() }", FileKind::Kt, ParseOptions { expand_bodies: false });
```

AGENTS.md, "The consumer contract", says what a consumer may rely on.

Licensed under the Apache License, Version 2.0; see NOTICE for the code it ports.
