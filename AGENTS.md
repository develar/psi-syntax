# kotlin-syntax: what an agent must know

A lossless Kotlin lexer and parser in Rust, ported from the Kotlin compiler's own. The goal is exact: for every input,
`dump::psi_to_string` prints what Kotlin 2.4.21's `DebugUtil.psiToString(file, true, false).trim()` prints, byte for
byte. Consumers: accio (code search; declarations, call sites, imports) and Branch Deck's `code-symbols` (diff
highlighting, navigation, file summaries). Both read the tree directly: there is no tree-sitter compatibility layer and
there must never be one.

## Sources to port (read them, port them faithfully)

Kotlin at 9180ce6a7a7d3627f88b424717583f28ddca052d (2.4.21), a sparse checkout at
`K=/Users/develar/.branch-deck/pool/tree-sitter-kotlin-8608ba7a/1/tree-sitter-kotlin/local/kotlin`. A file absent on
disk is read with `git -C $K show HEAD:<path>`.

| what | source |
|---|---|
| Tokens | `compiler/psi/psi-api/src/org/jetbrains/kotlin/lexer/KtTokens.java`, `KtKeywordToken.java`, `KtSingleValueToken`, `KtModifierKeywordToken` |
| Lexer | `compiler/psi/parser/src/org/jetbrains/kotlin/lexer/Kotlin.flex` (the JFlex spec; `_JetLexer.java` is its generated form), `KotlinLexer.java` |
| Element types | `compiler/psi/psi-api/src/org/jetbrains/kotlin/KtNodeTypes.java` (+ `KtStubElementTypes` names where the dump prints them) |
| Builder | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/SemanticWhitespaceAwarePsiBuilder*.java`, `TruncatedSemanticWhitespaceAwarePsiBuilder.java`, and IntelliJ's `PsiBuilderImpl` (`/Users/develar/.cache/accio-bench/intellij-community/platform/core-impl/src/com/intellij/lang/impl/PsiBuilderImpl.java`): markers, `precede`, `done`, `drop`, `rollbackTo`, `collapse`, `error`, `doneBefore`, `errorBefore`, edge (whitespace/comment) binders, `remapCurrentToken`, lookahead |
| Comment binding | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/KotlinWhitespaceAndCommentsBinders.kt`, plus the platform's default binders (`WhitespacesBinders`) |
| Parser | `compiler/psi/parser/src/org/jetbrains/kotlin/parsing/AbstractKotlinParsing.java`, `KotlinParsing.java`, `KotlinExpressionParsing.java`, `KotlinParser.java`, the token-stream patterns (`FirstBefore`, `LastBefore`, `AbstractTokenStreamPattern`, …), `ParseUtils.kt` |
| KDoc | `compiler/psi/parser/src/org/jetbrains/kotlin/kdoc/lexer/KDoc.flex`, `KDocLexer.java`, `kdoc/parser/KDocParser.java`, `KDocLinkParser.kt`, `compiler/psi/psi-api/src/org/jetbrains/kotlin/kdoc/lexer/KDocTokens.java`, `kdoc/parser/KDocElementTypes.java`: the dump expands a doc comment's inner tree |

Keep the Java structure: one Rust function per Java method, the same name in snake_case, the same order, so a diff
against the Java reads line by line. Port behaviour, not just shape: newline sensitivity, rollback, recovery sets,
error messages (the dump prints `PsiErrorElement:<message>`) and the exact element types.

## Layout

| module | holds |
|---|---|
| `src/tokens.rs` | `TokenKind` (every Kotlin and KDoc token, plus WHITE_SPACE, BAD_CHARACTER), its debug name as the dump prints it, the keyword sets (hard, soft, modifier) |
| `src/unicode.rs` | `[:letter:]`, `[:digit:]` (Kotlin.flex) and `[:jletter:]` (KDoc.flex) above ASCII, decoded from the character maps of `_JetLexer.java` / `_KDocLexer.java`; generated, do not edit |
| `src/lexer.rs`, `src/kdoc/lexer.rs` | the ports of Kotlin.flex and KDoc.flex (states, string templates, raw strings, `$$` prefixes, nested block comments, shebang) |
| `src/kdoc/parser.rs` | `KDocParser` and `KDocLinkParser`: the tree runs them on every doc comment and markdown link |
| `src/node_kind.rs` | `NodeKind`, the composite element types with their dump names (`FUN`, `CLASS`, `KtFile` root, `PsiErrorElement`) |
| `src/builder.rs` | the PsiBuilder port: token cursor that skips whitespace and comments, markers with rollback, the newline mode stack, edge binders; produces the tree |
| `src/tree.rs` | the lossless tree: `u32` ids in preorder over three columns (`kinds` u16: element type index plus leaf and unexpanded bits; `ranges` [start, end]; `links` [parent, subtree_end]), 18 bytes a node; a node's subtree is `id..subtree_end`, children in order, tokens as leaves; whitespace and comments are leaves where PSI puts them. `Parser` drives the builders: the file's, then a stack of chameleon builders, each expanding its block, lambda, doc comment or markdown link in place where `bind` meets it (heap, not recursion). The consumer API: see "The consumer contract" |
| `src/parsing/*.rs` | `abstract_parsing.rs`, `kotlin_parsing.rs`, `expression_parsing.rs`, `patterns.rs`: the parser ports; `kotlin_parser.rs`: the `KotlinParser` entry points |
| `src/dump.rs` | `psi_to_string(&Tree, file_name) -> String`: the exact PSI debug format |
| `src/bin/ksdump.rs` | `ksdump --list LIST --root ROOT --out OUT`: writes `OUT/<path>.psi.txt` per file like the oracle, and `ksdump --bench --list LIST --root ROOT [--threads N] [--walk] [--no-bodies] [--repeat K] [--file-stats OUT.tsv]`: parse throughput in MB/s (one thread or N), the cost of a preorder walk and of a linear scan, tree bytes per source MB, body-less parsing; `--features alloc-stats` adds allocations per file, `--features counters` the builder's calls per token. `ksdump --lex --bench` times the lexer alone; `ksdump --check-bodies` checks `parse_body` against the full parse |
| `src/counters.rs` | per-thread event counters (cursor calls, markers, rollbacks, lexed/reused/skipped tokens) behind the `counters` feature; without it every `add` compiles to nothing |
| `tests/goldens.rs` | every pair under `testdata/{psi,spec}`: parse the `.kt`/`.kts`, compare `psi_to_string` with the `.txt` (trailing whitespace trimmed); prints the pass count and the first differing line of each failure; `KS_STRICT=1` fails on any difference but the stale pair of `tests/common::KNOWN_STALE` |
| `tests/oracle.rs` | each `tests/oracle/NAME.kt` against the live PSI oracle's dump `NAME.txt` of it: inputs that once differed |
| `tests/lexer.rs`, `tests/kdoc.rs`, `tests/robustness.rs` | the goldens' leaf texts against our tokens; the goldens' `KDoc` subtrees against ours; arbitrary bytes parse losslessly without a panic, ids in preorder, navigation consistent with the parent links, 5000 nested lambdas and blocks on a 256 KB stack |
| `tests/bodies.rs` | body-less parsing on every golden: the full tree minus each body's interior, `parse_body` equal to the full subtree; `leaves()` and `descendants()` |

## The consumer contract

What accio and Branch Deck may rely on (`src/tree.rs`):

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
  a pass over call sites parses in full. Measured on the community files, one thread: 72.0 -> 144.5 MB/s, nodes
  63.7M -> 31.2M, tree 7.64 -> 4.25 bytes per source byte.
- Allocator: kotlin-syntax uses the system allocator and builds no C. Consumers that already build C (tree-sitter)
  should set `#[global_allocator] static A: mimalloc::MiMalloc` (mimalloc 0.1.52): measured before batch 2's buffer
  reuse at 243.2 -> 272.5 MB/s with 8 threads (+12%) and 460.1 -> 528.5 with 16 (+15%), no change on one thread;
  batch 2 (the per-thread `Parser`) itself measured 69.1 MB/s one thread and 917 MB/s with 16. Re-measure mimalloc
  on the current code before relying on the number.

## The dump format (`DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false)`)

First line `KtFile: <file name>`, then each node indented two spaces per depth: a composite prints its element type
(`PROPERTY`), an empty composite gets a child line `<empty list>`, a leaf prints `PsiElement(<token debug name>)('<text>')`,
whitespace `PsiWhiteSpace('<text>')`, a comment `PsiComment(EOL_COMMENT)('<text>')`, an error `PsiErrorElement:<message>`.
Texts escape `\n`, `\t`, `\r` and `\\`, `'` as `DebugUtil` does. The whole dump is trimmed.

## Oracles

1. `cargo test --release --test goldens`: the 789 vendored goldens of the compiler's own parsing tests (Apache-2.0,
   see NOTICE). The first gate; run it after every change.
2. The live PSI oracle (kotlin-compiler-embeddable 2.4.21) on real code:
   `java -Xss512m -Xmx8g -cp "$HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/lib/*" $HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/PsiOracle.java --list LIST --root ROOT --out OUT`
   writes `OUT/<path>.psi.txt` and `OUT/index.jsonl` (`path`, `psiErrors`, `firstError`). Ready dumps:
   `$HOME/.cache/kotlin-syntax/oracle/community-3caa0bf/` for the list `$HOME/.cache/kotlin-syntax/corpus/community-3caa0bf.list`
   over `/Users/develar/.cache/accio-bench/intellij-community` (55 501 files). Compare with
   `scripts/compare.py ORACLE_DIR OURS_DIR`.
3. The lexer oracle: `java -cp "$HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list LIST
   --root ROOT` prints a hash of each file's tokens (Kotlin, and KDoc inside each doc comment) as `ksdump --lex` does;
   `diff` the `cut -f1,2` of both. `scripts/lex-fuzz.py` makes mutated inputs for it.
4. Error recovery: `scripts/parse-fuzz.py LIST ROOT OUT [SEED]` mutates files at token boundaries (punctuation,
   keywords, modifiers) for the PSI oracle and `scripts/compare.py`; `scripts/lex-fuzz.py`'s inputs serve it too.
   `scripts/psi-reduce.py FILE OUT` shrinks a differing file to a small input; a fixed case goes to `tests/oracle/`
   (`NAME.kt` and the oracle's dump `NAME.txt`), which `cargo test --release --test oracle` compares.

Never edit a golden or an oracle dump to make a test pass. A difference is a bug in the port until the Java says
otherwise.

## Porting conventions (the builder and parser API)

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
- Every stub says `TODO(port): <File>.java:<line>`; port the method there, keep the order.

## Rules

- Dependencies: a well-maintained crate is better than a reinvented one when it is measurably as fast or faster
  and keeps the output byte-identical; each one is justified in its commit (what it replaces, the measured effect,
  its maintenance). Nothing that needs a C toolchain or pulls a large tree. No `unsafe` of our own, no `unwrap` on
  input-dependent paths: any byte sequence parses (invalid UTF-8 is lexed as BAD_CHARACTER bytes) and never panics.
- Speed is a goal: no allocation per token beyond the arena, no `String` per leaf (texts are ranges), no recursion
  depth that a 4 MB file of nested expressions can overflow without a guard (PSI's own limits apply).
- Measure speed with `scripts/ab.sh OLD_KSDUMP NEW_KSDUMP [ROUNDS] [BENCH ARGS]` (alternating runs, medians, load:
  the machine is shared) and profile with `scripts/profile.py` (macOS `sample`, symbolized inline-aware with
  `atos -i`: self time per function and per source line, inclusive time per source function).
- `scripts/gate.sh` prints PASS before every commit: clippy, `KS_STRICT=1 cargo test --release`, byte equality of
  all 55 501 community dumps and of the cached fuzz corpora with the PSI oracle, `ksdump --check-bodies` on the
  community files. A faster change that fails it is not a change.
- The default branch is `master`. Commit messages: imperative subject ≤ 72 characters, a body that says what the
  change does, its measured effect and that the gate passed.
