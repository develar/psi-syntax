# psi-syntax: what an agent must know

Lossless lexers and parsers in Rust, ported from the ones behind IntelliJ's PSI, a Cargo workspace:

| crate | holds |
|---|---|
| `crates/psi-builder` | the language-neutral half: IntelliJ's `PsiBuilderImpl` (markers, production, edge binders), the preorder column tree and its consumer API, the parse driver that expands chameleons in place, the dump's layout, token-set storage and lexer helpers, all generic over `psi_builder::Language` |
| `crates/kotlin-syntax` | Kotlin (package `kotlin-syntax` for consumers): tokens, lexers, node kinds, binders, the `SemanticWhitespaceAwarePsiBuilder` layer, the parsers, KDoc, and `Kotlin: Language` |
| `crates/java-syntax` | Java (package `java-syntax`): tokens, node types, the Java and JavaDoc lexers (complete), the binders, `Java: Language`, the dumps, and the parsers (see "Java") |
| `crates/ksdump` | the `ksdump` binary: dumps, benchmarks, lexes, checks bodies (`--lang kotlin\|java`, else by extension) |

The goal is exact: for every input, `kotlin_syntax::dump::psi_to_string` prints what Kotlin 2.4.21's
`DebugUtil.psiToString(file, true, false).trim()` prints, byte for byte. Consumers: accio (code search; declarations,
call sites, imports) and Branch Deck's `code-symbols` (diff highlighting, navigation, file summaries). Both read the
tree directly: there is no tree-sitter compatibility layer and there must never be one. `docs/DESIGN.md` describes the
architecture as built, its decisions, the measured numbers and the open items.

## Sources to port (read them, port them faithfully)

Kotlin at 9180ce6a7a7d3627f88b424717583f28ddca052d (2.4.21), a sparse checkout at
`K=/Users/develar/.branch-deck/pool/tree-sitter-kotlin-8608ba7a/1/tree-sitter-kotlin/local/kotlin`. A file absent on
disk is read with `git -C $K show HEAD:<path>`.

| what | source |
|---|---|
| Tokens | `compiler/psi/psi-api/src/org/jetbrains/kotlin/lexer/KtTokens.java`, `KtKeywordToken.java`, `KtSingleValueToken`, `KtModifierKeywordToken` |
| Lexer | `compiler/psi/parser/src/org/jetbrains/kotlin/lexer/Kotlin.flex` (the JFlex spec; `_JetLexer.java` is its generated form), `KotlinLexer.java` |
| Element types | `compiler/psi/psi-api/src/org/jetbrains/kotlin/KtNodeTypes.java` (+ `KtStubElementTypes` names where the dump prints them) |
| Builder | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/SemanticWhitespaceAwarePsiBuilder*.java`, `TruncatedSemanticWhitespaceAwarePsiBuilder.java` (in `kotlin-syntax`), and IntelliJ's `PsiBuilderImpl` (`/Users/develar/.cache/accio-bench/intellij-community/platform/core-impl/src/com/intellij/lang/impl/PsiBuilderImpl.java`, in `psi-builder`): markers, `precede`, `done`, `drop`, `rollbackTo`, `collapse`, `error`, `doneBefore`, `errorBefore`, edge (whitespace/comment) binders, `remapCurrentToken`, lookahead |
| Comment binding | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/KotlinWhitespaceAndCommentsBinders.kt`, plus the platform's default binders (`WhitespacesBinders`) |
| Parser | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/AbstractKotlinParsing.java`, `KotlinParsing.java`, `KotlinExpressionParsing.java`, `KotlinParser.java`, the token-stream patterns (`FirstBefore`, `LastBefore`, `AbstractTokenStreamPattern`, …), `ParseUtils.kt` |
| KDoc | `compiler/psi/parser/src/org/jetbrains/kotlin/kdoc/lexer/KDoc.flex`, `KDocLexer.java`, `kdoc/parser/KDocParser.java`, `KDocLinkParser.kt`, `compiler/psi/psi-api/src/org/jetbrains/kotlin/kdoc/lexer/KDocTokens.java`, `kdoc/parser/KDocElementTypes.java`: the dump expands a doc comment's inner tree |

Keep the Java structure: one Rust function per Java method, the same name in snake_case, the same order, so a diff
against the Java reads line by line. Port behaviour, not just shape: newline sensitivity, rollback, recovery sets,
error messages (the dump prints `PsiErrorElement:<message>`) and the exact element types.

### Java (lexers and parsers ported)

JetBrains/intellij-community at decc943674edfe7ecc620d2d60b61be38f876994: `java/java-syntax` (`_JavaLexer.flex`, `_JavaDocLexer.flex`,
`JavaParser` and its `*Parser.kt`, `JavaSyntaxElementType`, `JavaSyntaxTokenType`, `JavaDocSyntax*`), on the builder of
`platform/syntax/*` (`SyntaxTreeBuilderImpl`, `MarkerProduction`, `WhitespacesBinders`). Read them with
`git -C ~/.cache/psi-syntax/intellij-community show decc943674edfe7ecc620d2d60b61be38f876994:<path>`, never from the moving
working tree. The live oracle is the installed IntelliJ 2027.1 (build 271.SNAPSHOT, built from that revision):
`/Applications/Idea.app/Contents/plugins/java/lib/intellij.java.syntax.jar` with
`/Applications/Idea.app/Contents/lib/intellij.platform.syntax*.jar` and the Kotlin stdlib and util jars beside them.
The same rules hold as for Kotlin: one Rust function per Kotlin/Java method, the same names and order, error messages
verbatim. `java-syntax` implements `psi_builder::Language` as `kotlin-syntax` does.

| what | source (under `java/java-syntax/src/com/intellij/java/syntax/`) | port (`crates/java-syntax/src/`) |
|---|---|---|
| Tokens, element types | `element/JavaSyntaxTokenType.kt`, `JavaDocSyntaxTokenType.kt`, `JavaSyntaxElementType.kt`, `JavaDocSyntaxElementType.kt`, `JShellSyntaxElementType.kt`, `SyntaxElementTypes.kt`, `JavaDocSyntaxTokenTypes.kt` | `tokens.rs`, `node_kind.rs`, `syntax_element_types.rs` |
| Lexers | `lexer/_JavaLexer.flex`, `JavaLexer.kt`, `JavaTypeEscapeLexer.kt`, `_JavaDocLexer.flex`, `JavaDocLexer.kt` (`gen/.../_JavaLexer.kt`, `_JavaDocLexer.kt` for the Unicode tables) | `lexer.rs`, `doc_lexer.rs`, `unicode.rs` (`tools/java-unicode.py`) |
| Binders, lazy elements | `element/WhiteSpaceAndCommentSetHolder.kt`, `JavaWhitespaceOrCommentBindingPolicy.kt`, `element/lazyParser/*.kt`, `JavaSyntaxDefinition.kt`, `JavaDocParser.fakeCollapseRightBinder` | `builder.rs` (`Binder`), `language.rs`, `tree.rs` (`FileKind`, `Entry`, `LazyKind`) |
| Parsers | `parser/*.kt` | `parser/*.rs`, one file per Kotlin file; `JavaParser` wires them; the one unported method, `JavaParserUtil.stoppingBuilder(builder, condition)` (JavaParserUtil.kt:133), has no caller in intellij-community and no stub (`java_parser_util.rs` says why) |
| Messages, levels | `resources/messages/JavaSyntaxBundle.properties`, `pom/java/LanguageLevel.kt`, `JavaFeature.kt` | `bundle.rs`, `language_level.rs` |

### C# (pins and oracle)

dotnet/roslyn at 35d9211b841e7613c1d2f8f5af6d628ace696c4c (branch release/insiders, no tag contains it), the commit
NuGet `Microsoft.CodeAnalysis.CSharp` 5.9.0 is built from: the lexer and parser under
`src/Compilers/CSharp/Portable/Parser/`, the kinds and node shapes under `src/Compilers/CSharp/Portable/Syntax/`
(`SyntaxKind.cs`, `Syntax.xml`), messages in `src/Compilers/CSharp/Portable/CSharpResources.resx`. Read them with
`git -C ~/.cache/psi-syntax/roslyn show 35d9211b841e7613c1d2f8f5af6d628ace696c4c:<path>` (a blobless clone), never
from a working tree. Scope: full parses of `.cs` files at `LanguageVersion.Latest` (gated at Preview too),
`DocumentationMode.Parse`, diagnostics included; no scripts, preprocessor symbols, incremental parsing or node cache.

The oracle is that package on .NET SDK 10.0.403 (`~/.cache/psi-syntax/dotnet`, pinned by `global.json`, packages by
`packages.lock.json`): `tools/csharp-oracle/oracle.sh dump --list LIST --root ROOT --out OUT [--version latest|preview]
[--doc parse|none]` writes `OUT/<path>.tree.txt` and `index.jsonl`; `oracle.sh lex --list LIST --root ROOT [--doc ...]`
prints the lexer hash lines. `tools/csharp-oracle/README.md` is the normative dump format, the goldens' extraction and
the caches: `testdata/csharp/` (7051 goldens of Roslyn's parser and lexer tests, `manifest.tsv` with version, entry
and documentation mode), the corpora dotnet/roslyn at the pin, dotnet/runtime and dotnet/aspnetcore v10.0.14,
AvaloniaUI/Avalonia 12.1.4 under `~/.cache/psi-syntax/csharp-src/` with lists in `corpus/csharp-*.list`, dumps in
`oracle/csharp-*` (and `-preview`), lexer lines in `lex/csharp-*`, mutations and lexer fuzz in `fuzz/csharp-*`;
`tools/csharp-oracle/caches.sh` rebuilds them, `vendor-goldens.sh` the goldens. `scripts/compare.py` compares
`.tree.txt` dumps as it does `.psi.txt`.

## Layout

Paths of `kotlin-syntax` are under `crates/kotlin-syntax/`, of `psi-builder` under `crates/psi-builder/`.

| module | holds |
|---|---|
| `psi-builder` `src/language.rs` | `TreeLanguage` (what a tree needs: token and node types, the file kind, the error element), `Language: TreeLanguage` (what the builder and driver ask a language besides: binders, whitespace and comments, left-bound types, chameleons and bodies, the cursor's joined token, brackets, how a file and a chameleon are lexed and parsed), `TokenType` (with `NODE_BASE`, where node indices start), `NodeType`, the generic `ElementType<T, N>` |
| `psi-builder` `src/builder.rs` | the PsiBuilder port: token cursor that skips whitespace and comments, markers with rollback (the production a linked list, so `precede` and the other middle edits take constant time), the `WhitespacesAndCommentsBinder` contract, edge balancing, the nesting limits (a language's own through `Language` constants), the balanced-block and too-deep skips, block ends in amortized constant time (`balanced_block_end`); binds the tree |
| `psi-builder` `src/tree.rs` | the lossless tree: `u32` ids in preorder over three columns (`kinds` u16: element type index plus leaf, unexpanded, trailing and missing bits; `ranges` [start, end]; `links` [parent, subtree_end]), 18 bytes a node; a node's subtree is `id..subtree_end`, children in order, tokens as leaves; whitespace and comments are leaves where PSI puts them. `Parser<L>` drives the builders: the file's, then a stack of chameleon builders, each expanding its chameleon in place where `bind` meets it (heap, not recursion), through `Language::parse_chameleon` and its `Chameleon` (the body's file tokens when reusable, else relexed). `TreeBuilder<L>` builds a tree in preorder for a language whose parser does not run on `PsiBuilderImpl`. The consumer API: see "The consumer contract" |
| `psi-builder` `src/dump.rs`, `src/token_set.rs`, `src/lexer.rs`, `src/counters.rs` | the dump's layout (`psi_to_string_with`, a language labels each node; `fix_white_spaces`); `ElementTypeSet`, a `TokenSet`'s bits; `Lexed<T>` (types, starts, plain bits), `char_at`, `in_ranges`; the builder's and driver's event counters |
| `src/tokens.rs` | `TokenKind` (every Kotlin and KDoc token, plus WHITE_SPACE, BAD_CHARACTER), its debug name as the dump prints it, the keyword sets (hard, soft, modifier) |
| `src/unicode.rs` | `[:letter:]`, `[:digit:]` (Kotlin.flex) and `[:jletter:]` (KDoc.flex) above ASCII, decoded from the character maps of `_JetLexer.java` / `_KDocLexer.java`; generated, do not edit |
| `src/lexer.rs`, `src/kdoc/lexer.rs` | the ports of Kotlin.flex and KDoc.flex (states, string templates, raw strings, `$$` prefixes, nested block comments, shebang) |
| `src/kdoc/parser.rs` | `KDocParser` and `KDocLinkParser`: the tree runs them on every doc comment and markdown link |
| `src/node_kind.rs` | `NodeKind`, the composite element types with their dump names (`FUN`, `CLASS`, `KtFile` root, `PsiErrorElement`) |
| `src/language.rs` | `Kotlin`, the `Language` impl: `KotlinParserDefinition`'s whitespace and comments and file type, the lazy element types (BLOCK, LAMBDA_EXPRESSION, KDoc, markdown link) and how each is lexed and parsed |
| `src/builder.rs` | Kotlin's binders (`Binder`: the platform defaults, `PrecedingCommentsBinder`, `KotlinWhitespaceAndCommentsBinders`, ...), `SemanticWhitespaceAwarePsiBuilderImpl` (newline mode and join stacks, `getJoinedTokenType`) and its adapters seen through the `SemanticWhitespaceAwarePsiBuilder` handle; re-exports the builder for Kotlin's types (`PsiBuilderImpl`, `Marker`, `Nesting`, the limits) |
| `src/tree.rs` | `Tree`, `Parser` (`psi_builder`'s over `Kotlin`), `FileKind`, `LazyKind` |
| `src/parsing/*.rs` | `abstract_parsing.rs`, `kotlin_parsing.rs`, `expression_parsing.rs`, `patterns.rs`: the parser ports; `kotlin_parser.rs`: the `KotlinParser` entry points |
| `src/dump.rs` | `psi_to_string(&Tree, file_name) -> String`: the exact PSI debug format (Kotlin's labels over `psi_builder::dump`) |
| `src/counters.rs` | the parser's own event counters (semantic layer, chameleons, KDoc tokens, soft keywords) and `ALL`, every counter of a parse in the order `ksdump` prints them |
| `java-syntax` `src/lexer.rs`, `src/doc_lexer.rs`, `src/unicode.rs` | `JavaLexer` (by hand: whitespace, comments, literals, text blocks, string templates, unicode escapes) over `FlexJavaLexer` (the rules of `_JavaLexer.flex`), `JavaTypeEscapeLexer`; `JavaDocLexer` (merging adapter, `AsteriskStripperLexer`) over `FlexJavaDocLexer` (every state of `_JavaDocLexer.flex`, longest match, earlier rule on a tie); `[:jletter:]`, `[:jletterdigit:]` above ASCII, generated |
| `java-syntax` `src/builder.rs` | `SyntaxTreeBuilder` (the builder as a Java parser method receives it, with `JavaParserUtil.stoppingBuilder`'s stop offset), the `SyntaxBuilderUtil` helpers (`advance`, `expect`, `expect_set`, `drop`, `raw_token_text`, `parse_block_lazy`), Java's `Binder`s and `WhiteSpaceAndCommentSetHolder` |
| `java-syntax` `src/language.rs`, `src/tree.rs` | `Java`, the `Language` impl (whitespace and comments, left-bound types, the five lazy element types and their lexers and parsers, `parse_fragment`); `Tree`, `Parser`, `FileKind { level, entry }` (`Entry::File` or one of the 13 parser-partial entries), `LazyKind` |
| `java-syntax` `src/parser/*.rs` | `java_parser.rs` (`JavaParser`, `JavaSyntaxDefinition.parse`), `java_parser_util.rs` and `java_keywords.rs` (ported), `file_parser.rs`, `module_parser.rs`, `declaration_parser.rs`, `statement_parser.rs`, `expression_parser.rs` (ported: delegates), `pratt_expression_parser.rs`, `reference_parser.rs`, `pattern_parser.rs`, `java_doc_parser.rs`, `jshell_parser.rs`: every method a function, all ported but `JavaParserUtil.stoppingBuilder(builder, condition)`, which nothing calls (`java_parser_util.rs` says why) |
| `java-syntax` `src/dump.rs` | `psi_to_string` (`DebugUtil.psiToString`: PSI class names and their `toString()`, the labels the goldens show) and `node_tree_to_string` (`DebugUtil.nodeTreeAsElementTypeToString`, the `_node.txt` goldens) |
| `java-syntax` `tests/lexer.rs`, `tests/goldens.rs`, `tests/doc_comments.rs`, `tests/robustness.rs` | the leaf texts of all 1036 Java goldens against our tokens (adjacent tokens joined where the parser collapses them, reference and type holders relexed); every golden parsed at its level through its entry, both dumps against `NAME.txt` and `NAME_node.txt`; `JavaDocParser` alone, each golden parsed flat and its `PsiDocComment` subtrees compared with the golden's (`JAVA_STRICT=1` or `KS_STRICT=1` fails on any difference); 80 recursive forms nested 100 000 deep, 44 chains and lists of 100 000 and 40 random mixes on a 1 MB stack, random bytes and fragments, every golden under nesting limits 1 to 13 (lossless, `parse_body` exact) |
| `crates/ksdump/src/main.rs` | `ksdump --list LIST --root ROOT --out OUT`: writes `OUT/<path>.psi.txt` per file like the oracle, and `ksdump --bench --list LIST --root ROOT [--threads N] [--walk] [--no-bodies] [--repeat K] [--file-stats OUT.tsv]`: parse throughput in MB/s (one thread or N), the cost of a preorder walk and of a linear scan, tree bytes per source MB, body-less parsing; `--features alloc-stats` adds allocations per file, `--features counters` the builder's calls per token. `ksdump --lex --bench [--docs]` times the file lexer alone (Kotlin's or Java's, a list of one language), with `--docs` also the doc-comment lexer over every doc comment; `--lex` hashes the tokens `tokenize_into` makes, as a parse lexes them; `ksdump --check-bodies` checks that the leaves are the text and `parse_body` against the full parse; `--nesting-limit N` parses with that limit (the too-deep paths on ordinary files); `--lang kotlin|java` (else `.java` is Java, anything else Kotlin); `--lex` lexes Java files (a list line may be `PATH<TAB>LEVEL`), `--out` dumps them (a list line as `JavaPsiOracle.java` reads it, `--node` adds `.node.txt`), `--bench` takes a list of Java files (`PATH[<TAB>LEVEL]`, never mixed with Kotlin), `--check-bodies` takes one as `--out` does; C# (`--lang csharp` or `.cs`): `--out` and `--lex` take list lines as `tools/csharp-oracle` reads them, `--version V` and `--doc parse|none|diagnose` fill their empty columns, `--lex --bench` times the lexer alone. Features `counters` (both crates' per-thread counters; without it every `add` compiles to nothing) and `alloc-stats` |
| `tests/goldens.rs` | every pair under `testdata/{psi,spec}`: parse the `.kt`/`.kts`, compare `psi_to_string` with the `.txt` (trailing whitespace trimmed); prints the pass count and the first differing line of each failure; `KS_STRICT=1` fails on any difference but the stale pair of `tests/common::KNOWN_STALE` |
| `tests/oracle.rs` | each `tests/oracle/NAME.kt` against the live PSI oracle's dump `NAME.txt` of it: inputs that once differed |
| `tests/lexer.rs`, `tests/kdoc.rs`, `tests/robustness.rs` | the goldens' leaf texts against our tokens; the goldens' `KDoc` subtrees against ours; arbitrary bytes parse losslessly without a panic, ids in preorder, navigation consistent with the parent links; 22 recursive forms nested 100 000 deep and 18 left-associative chains of 100 000 operators on a 1 MB stack, each well under a second; every golden under nesting limits 1 to 13, lossless and with `parse_body` exact |
| `tests/bodies.rs` | body-less parsing on every golden: the full tree minus each body's interior, `parse_body` equal to the full subtree; `leaves()` and `descendants()` |

## The consumer contract

What accio and Branch Deck may rely on (`psi-builder` `src/tree.rs`, named for Kotlin by `kotlin_syntax::{Tree, Parser}`):

- One `Parser` per thread (`Parser::new()`, then `parser.parse(text, FileKind)` per file): it keeps every buffer of a
  parse and reuses it; `Tree::parse` makes a fresh one per call. `Parser` is `Send`; `Tree` is `Send + Sync`, owns
  its copy of the text (`Arc<[u8]>`) and has no lifetime, so a tree can be handed to other threads (a const
  assertion in `tree.rs` keeps this true).
- Ids are preorder: `0..len()` is document order, the subtree of `id` is `id..subtree_end(id)`. `root`, `kind`,
  `token_kind`, `node_kind`, `is_leaf`, `range`, `text`, `parent`, `first_child`, `last_child`, `next_sibling`,
  `children`, `first_child_of_kind`, `preorder` (with depth), `tokens`, `error_message`; `descendants(id)` is the
  id range, so "every `CALL_EXPRESSION` under X" is a scan of one column.
- Highlighting reads `leaves()`: each token leaf's `(TokenKind, range)` in document order. Not the lexer: soft keywords
  are remapped by the parser (`IDENTIFIER` becomes `by`, `get`, `data`, ...) and `?.`, `?:`, `!!` are joined by the
  parser from two lexer tokens, so only the tree's leaves are exact.
- Body-less parsing: `parser.parse_with_options(text, kind, ParseOptions { expand_bodies: false })` leaves every lazily
  parsed block and lambda (what PSI builds as a chameleon) a childless node with its kind and range,
  `is_unexpanded(id)`; doc comments are still expanded. `tree.parse_body(id)` (or `parser.parse_body(&tree, id,
  options)`) parses one on demand into its own `Tree` over the same source, exactly the full parse's subtree there
  (the gate checks all 229 960 bodies of the community files). For declaration-only passes, summaries, navigation;
  a pass over call sites parses in full. Measured on the community files, one thread, 72c8c11: Kotlin
  111.3 -> 231.8 MB/s, nodes 63.7M -> 31.2M, tree 7.64 -> 4.26 bytes per source byte; Java 120.0 -> 265.2 MB/s,
  nodes 85.3M -> 38.6M, tree 8.12 -> 4.24.
- Nesting: a parse runs in a 1 MB stack (a release build; the deepest form measured needs 0.64 MB). It counts the
  levels of its recursive entry points (expressions, types, member declarations, type argument lists, annotations,
  parenthesized types; a lazy block or lambda adds 4 to where it sits) up to `builder::NESTING_LIMIT` (1000), at most
  `TYPE_ARGUMENTS_LIMIT` (32) nested type argument lists and `REPARSED_LIMIT` (10) nested annotations and
  parenthesized types per block. A construct past a limit is an `ERROR_ELEMENT` with the message
  `builder::TOO_DEEPLY_NESTED`, "Code is too deeply nested", over the rest of the construct (up to the bracket that
  closes it); a block or lambda past it is one such error over all its tokens. The tree stays lossless, in preorder
  and deterministic. PSI has no defined result there (kotlinc overflows its stack); below the limits the tree is PSI's
  (the deepest community file reaches 62 levels, 5 type argument lists, 4 annotations or parenthesized types).
  Java (`java_syntax::Java`'s `NESTING_LIMIT` 1200, `TYPE_ARGUMENTS_LIMIT` 32, `REPARSED_LIMIT` 5, the same error) also
  runs in a 1 MB stack (the deepest form at the limit needs 772 KB): it counts expressions, unary operands,
  statements, declarations, array initializers, types, type argument lists, annotations, patterns and inline doc tags;
  a code block adds 4 to where it sits. Below the limits the tree is PSI's (the deepest community file,
  `Parenth2.java`, reaches 1010 levels, 4 type argument lists, 4 annotations). See DESIGN, "Java nesting".
- Allocator: psi-syntax uses the system allocator and builds no C. mimalloc 0.1.52 was +12-15 % at 8 and 16
  threads before batch 2's buffer reuse; on the current code it changes no instructions and its wall time is within
  noise at 1 and 16 threads (DESIGN, "Build and toolchain"), so a consumer needs no allocator for the parse.
- Build: the consumer's release profile compiles psi-syntax. Fat LTO and one codegen unit, as here; a profile-guided
  build (`scripts/pgo.sh` is the recipe: instrument, run over a few MB of Kotlin and Java, merge, rebuild with the
  same rustc; in a consumer's own build `RUSTFLAGS=-Cprofile-generate=DIR`, a run, `llvm-profdata merge`,
  `RUSTFLAGS=-Cprofile-use=merged.profdata`) cuts 12 % of Kotlin's and 13 % of Java's instructions on 72c8c11
  (goldens as training; ×1.10 and ×1.13 wall on one thread; other real code as training did 1-4 points better on the
  hints commit's first base) with byte-identical trees; `panic = "abort"` another 1-2 %. Measured in DESIGN, "Build
  and toolchain" and "Current".

## The dump format (`DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false)`)

First line `KtFile: <file name>`, then each node indented two spaces per depth: a composite prints its element type
(`PROPERTY`), an empty composite gets a child line `<empty list>`, a leaf prints `PsiElement(<token debug name>)('<text>')`,
whitespace `PsiWhiteSpace('<text>')`, a comment `PsiComment(EOL_COMMENT)('<text>')`, an error `PsiErrorElement:<message>`.
Texts escape `\n`, `\t`, `\r` and `\\`, `'` as `DebugUtil` does. The whole dump is trimmed.

## Oracles

1. `cargo test --release --test goldens`: the 789 vendored goldens of the compiler's own parsing tests (Apache-2.0,
   see NOTICE). The first gate; run it after every change.
2. The live PSI oracle (kotlin-compiler-embeddable 2.4.21) on real code:
   `java -Xss512m -Xmx8g -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" $HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/PsiOracle.java --list LIST --root ROOT --out OUT`
   writes `OUT/<path>.psi.txt` and `OUT/index.jsonl` (`path`, `psiErrors`, `firstError`). Ready dumps:
   `$HOME/.cache/psi-syntax/oracle/community-3caa0bf/` for the list `$HOME/.cache/psi-syntax/corpus/community-3caa0bf.list`
   over `/Users/develar/.cache/accio-bench/intellij-community` (55 501 files). Compare with
   `scripts/compare.py ORACLE_DIR OURS_DIR`.
3. The lexer oracle: `java -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list LIST
   --root ROOT` prints a hash of each file's tokens (Kotlin, and KDoc inside each doc comment) as `ksdump --lex` does;
   `diff` the `cut -f1,2` of both. `scripts/lex-fuzz.py` makes mutated inputs for it.
4. Error recovery: `scripts/parse-fuzz.py LIST ROOT OUT [SEED]` mutates files at token boundaries (punctuation,
   keywords, modifiers) for the PSI oracle and `scripts/compare.py`; `scripts/lex-fuzz.py`'s inputs serve it too.
   `scripts/psi-reduce.py FILE OUT` shrinks a differing file to a small input; a fixed case goes to
   `crates/kotlin-syntax/tests/oracle/`
   (`NAME.kt` and the oracle's dump `NAME.txt`), which `cargo test --release --test oracle` compares.

Java: `testdata/java/` (1036 goldens of IntelliJ's Java parsing tests, `manifest.tsv` with level, entry and trim per
test) and the Java PSI oracle `tools/java-oracle/JavaPsiOracle.java` (see `tools/java-oracle/README.md`: cached dumps of
the 68 263 community `.java` files, 19 116 golden mutations and 20 000 lexer fuzz inputs). The Java lexer oracle
`java -cp "$(tools/java-oracle/fetch-jars.sh)/*" tools/java-oracle/JavaLexOracle.java --list LIST --root ROOT` prints
the lines `ksdump --lex` prints for `.java` files (JavaDoc tokens of each doc comment inside `[` `]`);
`tools/java-oracle/lex_fuzz.py` makes lexer fuzz inputs for it. Cached lines: `$HOME/.cache/psi-syntax/lex/`.

Never edit a golden or an oracle dump to make a test pass. A difference is a bug in the port until the Java says
otherwise.

## Porting conventions (the builder and parser API)

- What is the platform's goes to `psi-builder`, generic over `L: Language`; what is the language's goes to its crate,
  and reaches the builder only through `Language` (a new need of a parser is a new `Language` item or a new generic
  builder method, never a language's token inside `psi-builder`). Monomorphized, the generic code is the code it was:
  the split was measured at no cost (DESIGN, "Measurements").

- `AbstractKotlinParsing` is a trait implemented by `KotlinParsing<'b>` and `KotlinExpressionParsing<'p, 'b>`; its
  helpers are provided methods (`self.at(LBRACE)`, `self.expect_with(RPAR, "Expecting ')'")`). `myBuilder` is
  `self.my_builder()`, a `SemanticWhitespaceAwarePsiBuilder` (the Java builder seen through its adapters).
- `KotlinExpressionParsing` is a borrow of its `KotlinParsing`: `myExpressionParsing.f()` is
  `self.my_expression_parsing().f()`, `myKotlinParsing.g()` is `self.my_kotlin_parsing.g()`.
- A marker is a `Copy` handle whose methods take the builder: `marker.done(self.my_builder(), FUN)`,
  `marker.precede(self.my_builder())`, `marker.error(self.my_builder(), "...")`, `rollback_to`, `drop`, `collapse`,
  `done_before`, `error_before`, `set_custom_edge_token_binders(b, Some(left), None)`.
- A token type is `Option<TokenKind>` (`null` = `None`) and compares with a token directly: `self.tt() == LBRACE`. A
  `TokenSet` is a `const`, `TokenSet::create(&[..])`, `or_set`, `and_set`, `and_not`; `contains` takes a token, a node
  type or an `Option`. Composite types are `NodeKind`; an `IElementType` result is `Option<NodeKind>`.
- Overloads get suffixed names: the fewest-argument form keeps the name, the others `_with` (or a descriptive suffix,
  e.g. `expect_with_recovery`, `parse_type_ref_with_intersections`), documented on each function.
- Every stub says `TODO(port): <File>.java:<line>` (Java: `<File>.kt:<line>`); port the method there, keep the order.
- Java: a parser method's `builder: SyntaxTreeBuilder` is `builder: &mut SyntaxTreeBuilder<'_>`
  (`java_syntax::builder`), its sub-parsers views reached through `self.my_parser` (`JavaParser`); a parameter named
  `type` is `element_type`; a Kotlin method that reaches the builder only through a marker takes the builder first
  (`java_parser_util::done(builder, marker, NodeKind::CLASS, level)`); a default parameter gives the short name the
  default and `_with` the full form (`parse_code_block` / `parse_code_block_with`). Token parameters are
  `Option<TokenKind>`, node types `NodeKind`, either `ElementType`.

## Rules

- Dependencies: a well-maintained crate is better than a reinvented one when it is measurably as fast or faster
  and keeps the output byte-identical; each one is justified in its commit (what it replaces, the measured effect,
  its maintenance). Nothing that needs a C toolchain or pulls a large tree. No `unsafe` of our own, no `unwrap` on
  input-dependent paths: any byte sequence parses (invalid UTF-8 is lexed as BAD_CHARACTER bytes) and never panics.
- Speed is a goal: no allocation per token beyond the arena, no `String` per leaf (texts are ranges), no recursion
  depth that a 4 MB file of nested expressions can overflow without a guard (PSI's own limits apply).
- `scripts/pgo.sh OUT_DIR [KOTLIN_LIST KOTLIN_ROOT JAVA_LIST JAVA_ROOT]` builds a profile-guided ksdump (training on
  the vendored goldens by default) and leaves the merged profile for a consumer's build.
- Measure speed with `scripts/ab.sh OLD_KSDUMP NEW_KSDUMP [ROUNDS] [BENCH ARGS]` (alternating runs, medians, load:
  the machine is shared) and profile with `scripts/profile.py` (macOS `sample`, symbolized inline-aware with
  `atos -i`: self time per function and per source line, inclusive time per source function).
- `scripts/gate.sh` prints PASS before every commit: clippy and `KS_STRICT=1 cargo test --release` over the workspace, byte equality of
  all 55 501 community dumps and of the cached fuzz corpora with the PSI oracle, `ksdump --check-bodies` on the
  community files, and again with `--nesting-limit 3` on the community files and the mutations, and the Java lexers
  against the Java lexer oracle on the community `.java` files, the golden mutations and the lexer fuzz, and both Java
  dumps (`.psi.txt`, `.node.txt`) of the same three corpora against the Java PSI oracle's, and `ksdump --check-bodies`
  on the community `.java` files and with `--nesting-limit 3` on them and the golden mutations, and the C# lexer
  (`--doc none`, and `--doc parse` with the documentation comment parser) against `tools/csharp-oracle`'s cached lexer
  lines on its four corpora, mutations and lexer fuzz. A faster change that fails it is not a change.
- Java speed: `KS_LIST=$HOME/.cache/psi-syntax/corpus/java-community-3caa0bf.list scripts/ab.sh OLD NEW` (the community
  `.java` files); DESIGN, "Java parse throughput", has the numbers and how tree-sitter-java was compared.
- The default branch is `master`. Commit messages: imperative subject ≤ 72 characters, a body that says what the
  change does, its measured effect and that the gate passed.
