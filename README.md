# psi-syntax

Lossless lexers and parsers in Rust, ported method by method from the ones behind IntelliJ's PSI and from Roslyn,
whose trees print exactly what the originals print. Crates of [accio](../README.md)'s Cargo workspace, which reads
them (`crates/extract`); Branch Deck depends on them as a revision of JetBrains/accio:

- `psi-builder`: the language-neutral half, IntelliJ's `PsiBuilder` (markers, production, edge binders), the preorder
  column tree and the parse driver that expands lazily parsed elements in place;
- `kotlin-syntax`: Kotlin, ported from the Kotlin compiler's own (`Kotlin.flex`, `KotlinParsing`,
  `KotlinExpressionParsing`, KDoc), its tree `DebugUtil.psiToString`'s;
- `java-syntax`: Java, ported from IntelliJ's `java-syntax`: the Java and JavaDoc lexers and the parsers, equal to
  IntelliJ's Java PSI on every file tested;
- `csharp-syntax`: C#, ported from Roslyn 5.9.0 (`Lexer`, `DirectiveParser`, `LanguageParser`,
  `DocumentationCommentParser`): Roslyn's syntax tree with its trivia, missing tokens and diagnostics, equal to
  Roslyn's on dotnet/roslyn, dotnet/runtime, dotnet/aspnetcore, Avalonia and Roslyn's own parser tests;
- `ksdump`: dumps, benchmarks and lexes listed files like the oracles.

See AGENTS.md for the porting rules and the oracles, docs/DESIGN.md for the architecture and its measurements.

```rust
use kotlin_syntax::{FileKind, ParseOptions, Parser};

let mut parser = Parser::new(); // one per thread; reuses its buffers from file to file
let tree = parser.parse(b"fun f() = 1", FileKind::Kt); // Tree: Send + Sync, ids in preorder
for (token, range) in tree.leaves() { /* exact highlighting: soft keywords and ?. ?: !! as the parser sees them */ }
let decls = parser.parse_with_options(b"fun f() { g() }", FileKind::Kt, ParseOptions { expand_bodies: false });
```

```rust
use csharp_syntax::{FileKind, LanguageVersion, Parser};

let mut parser = Parser::new(); // one per thread, as for Kotlin and Java
let tree = parser.parse(b"class C { void M() { } }", FileKind::default()); // LanguageVersion.Latest
let old = parser.parse(b"class C { int x = 1_000; }", FileKind::file(LanguageVersion::CSharp6));
for d in old.diagnostics() { /* d.id() "CS8059", d.start..d.end in bytes, "Feature 'digit separators' is not ..." */ }
```

AGENTS.md, "The consumer contract", says what a consumer may rely on.

Licensed under the Apache License, Version 2.0 (unlike the rest of accio, MIT); see NOTICE for the code it ports.
Its history before the move into accio, from github.com/develar/psi-syntax (archived), is replayed here as a line.
